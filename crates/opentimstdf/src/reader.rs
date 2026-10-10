use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};

use crate::calibration::{
    BundleCalibration, Calibration, MzCalibrationModel, MzCalibrationStatus, MzConversion,
    TablesMzModel,
};
use crate::codec::{checked_block_len, decode_codec1, decode_codec2, frame_from_row};
use crate::error::{Error, Result};
use crate::types::{
    DiaFrameWindows, DiaWindow, Frame, Metadata, PasefMsMsInfo, Peak, Precursor, PrmMsMsInfo,
    PrmTarget,
};

/// Positioned read without touching the file's seek cursor: `pread` on
/// Unix, `ReadFile` with an explicit offset on Windows.
#[cfg(unix)]
fn positioned_read(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn positioned_read(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

/// Reads exactly `buf.len()` bytes from `file` starting at `offset`,
/// without touching the file's seek cursor. Loops on short reads (neither
/// the POSIX `pread` contract nor Windows `ReadFile` guarantee a single
/// call fills the buffer).
fn read_at_exact(file: &File, mut offset: u64, mut buf: &mut [u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        match positioned_read(file, buf, offset) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "failed to fill whole buffer",
                ))
            }
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// One `.d/` (TDF) bundle on disk.
///
/// `conn` is behind a `Mutex` (rusqlite's `Connection` is `!Sync` because of
/// its internal statement cache) so `&Reader` can be shared across threads;
/// `decode_peaks` never touches it, so concurrent frame decoding never
/// contends on that lock, only the (cheap, infrequent) SQL metadata lookups
/// do.
pub struct Reader {
    bundle_dir: PathBuf,
    conn: Mutex<Connection>,
    compression_type: u32,
    /// Cached once at `open()` so `decode_peaks_codec1` never has to touch
    /// `conn` on the per-frame decode path.
    max_num_peaks_per_scan: u32,
    tdf_bin: File,
    /// Length of `analysis.tdf_bin`, read once at `open()` so the per-frame
    /// decode path does not issue an `fstat` for every frame. The bundle is
    /// treated as immutable while the reader is open.
    tdf_bin_len: u64,
}

impl Reader {
    pub fn open<P: AsRef<Path>>(bundle_dir: P) -> Result<Self> {
        let bundle_dir = bundle_dir.as_ref().to_path_buf();
        let tdf = bundle_dir.join("analysis.tdf");
        if !tdf.exists() {
            return Err(Error::MissingFile(tdf));
        }
        let tdf_bin_path = bundle_dir.join("analysis.tdf_bin");
        let tdf_bin = File::open(&tdf_bin_path).map_err(|_| Error::MissingFile(tdf_bin_path))?;
        let tdf_bin_len = tdf_bin.metadata()?.len();
        let conn = Connection::open_with_flags(&tdf, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let raw_ct: String = conn.query_row(
            "SELECT Value FROM GlobalMetadata WHERE Key = 'TimsCompressionType'",
            [],
            |row| row.get(0),
        )?;
        let compression_type: u32 = raw_ct
            .trim()
            .parse()
            .map_err(|_| Error::UnsupportedCodec(raw_ct.clone()))?;
        // Codec-1-only metadata: some codec-2 bundles omit this key entirely,
        // so a missing row must default to 0 rather than fail Reader::open.
        let max_num_peaks_per_scan: u32 = conn
            .query_row(
                "SELECT Value FROM GlobalMetadata WHERE Key='MaxNumPeaksPerScan'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        Ok(Reader {
            bundle_dir,
            conn: Mutex::new(conn),
            compression_type,
            max_num_peaks_per_scan,
            tdf_bin,
            tdf_bin_len,
        })
    }

    pub fn compression_type(&self) -> u32 {
        self.compression_type
    }

    pub fn bundle_dir(&self) -> &std::path::Path {
        &self.bundle_dir
    }

    /// Key-value metadata from `GlobalMetadata`: schema version, instrument, software, codec.
    pub fn metadata(&self) -> Result<Metadata> {
        fn meta(conn: &Connection, key: &str) -> Result<String> {
            Ok(conn.query_row(
                "SELECT Value FROM GlobalMetadata WHERE Key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )?)
        }
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let schema_major: u32 = meta(&conn, "SchemaVersionMajor")
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or(0);
        let schema_minor: u32 = meta(&conn, "SchemaVersionMinor")
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or(0);
        let instrument_name = meta(&conn, "InstrumentName").unwrap_or_default();
        let acquisition_software = meta(&conn, "AcquisitionSoftware").unwrap_or_default();
        let acquisition_software_version =
            meta(&conn, "AcquisitionSoftwareVersion").unwrap_or_default();
        let acquisition_date_time = meta(&conn, "AcquisitionDateTime").ok();
        Ok(Metadata {
            schema_version_major: schema_major,
            schema_version_minor: schema_minor,
            instrument_name,
            acquisition_software,
            acquisition_software_version,
            compression_type: self.compression_type,
            acquisition_date_time,
        })
    }

    /// Instrument serial number from `GlobalMetadata.InstrumentSerialNumber`.
    /// `None` when the key is absent or blank.
    pub fn instrument_serial_number(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let value: Option<String> = conn
            .query_row(
                "SELECT Value FROM GlobalMetadata WHERE Key = 'InstrumentSerialNumber'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        Ok(value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()))
    }

    /// Calibration for the `MzCalibration` row referenced by the most frames.
    ///
    /// Same as `self.bundle_calibration()?.primary()`. In a bundle whose
    /// frames reference more than one `MzCalibration` row (for example
    /// dual-polarity runs), convert each frame with
    /// [`BundleCalibration::for_frame`] instead.
    pub fn calibration(&self) -> Result<Calibration> {
        Ok(*self.bundle_calibration()?.primary())
    }

    /// Which TOF -> m/z model this bundle uses (see
    /// [`Reader::bundle_calibration`]).
    pub fn mz_calibration_model(&self) -> Result<MzCalibrationModel> {
        Ok(self.bundle_calibration()?.status.model)
    }

    /// The m/z model this bundle uses and, for the range fallback, why the
    /// calibration tables could not be used.
    pub fn mz_calibration_status(&self) -> Result<MzCalibrationStatus> {
        Ok(self.bundle_calibration()?.status)
    }

    /// Build the calibration for every `MzCalibration` row the bundle's
    /// frames reference (`docs/docs/format/04-calibration.md`).
    ///
    /// m/z uses [`TablesMzModel`] built from each referenced
    /// `MzCalibration` row. When the table is absent, a referenced row is
    /// missing, or any referenced row is unusable (`ModelType` other than 1,
    /// non-zero `C3`, non-numeric or out-of-range values), the whole bundle
    /// uses the range fallback instead, the reason is stored in
    /// [`MzCalibrationStatus::fallback_reason`], and a warning is logged
    /// through the `log` crate.
    ///
    /// 1/K0 is linear between `GlobalMetadata.OneOverK0AcqRangeUpper` (scan
    /// 0) and `OneOverK0AcqRangeLower` (scan `MAX(Frames.NumScans)`).
    ///
    /// Errors when the mobility range metadata is invalid, or when the
    /// range fallback is needed and the m/z range metadata is invalid.
    pub fn bundle_calibration(&self) -> Result<BundleCalibration> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;

        let (im_intercept, im_slope) = mobility_calibration(&conn)?;
        let tof_max: Option<u32> =
            global_meta(&conn, "DigitizerNumSamples")?.and_then(|v| v.trim().parse().ok());

        match tables_mz_models(&conn, tof_max) {
            Ok((primary_id, rows)) => {
                let by_row: BTreeMap<u32, Calibration> = rows
                    .into_iter()
                    .map(|(id, m)| {
                        let c = Calibration {
                            mz: MzConversion::Tables(m),
                            im_intercept,
                            im_slope,
                        };
                        (id, c)
                    })
                    .collect();
                let primary = by_row[&primary_id];
                Ok(BundleCalibration::new(
                    MzCalibrationStatus {
                        model: MzCalibrationModel::Tables,
                        fallback_reason: None,
                    },
                    primary,
                    by_row,
                ))
            }
            Err(reason) => {
                let (intercept, slope) = range_mz_calibration(&conn).map_err(|e| match e {
                    Error::CorruptFrame(id, msg) => Error::CorruptFrame(
                        id,
                        format!("{msg} (calibration tables unusable: {reason})"),
                    ),
                    other => other,
                })?;
                log::warn!(
                    "{}: m/z uses the acquisition-range fallback model, which can be off \
                     by hundreds of ppm: {reason}",
                    self.bundle_dir.display()
                );
                let primary = Calibration {
                    mz: MzConversion::RangeFallback { intercept, slope },
                    im_intercept,
                    im_slope,
                };
                Ok(BundleCalibration::new(
                    MzCalibrationStatus {
                        model: MzCalibrationModel::RangeFallback,
                        fallback_reason: Some(reason),
                    },
                    primary,
                    BTreeMap::new(),
                ))
            }
        }
    }

    /// The range-fallback calibration, built even when the calibration
    /// tables are usable. For comparing the two models; readers should use
    /// [`Reader::bundle_calibration`].
    pub fn range_fallback_calibration(&self) -> Result<Calibration> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let (im_intercept, im_slope) = mobility_calibration(&conn)?;
        let (intercept, slope) = range_mz_calibration(&conn)?;
        Ok(Calibration {
            mz: MzConversion::RangeFallback { intercept, slope },
            im_intercept,
            im_slope,
        })
    }

    pub fn frame(&self, frame_id: u32) -> Result<Frame> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let frame = conn.query_row(
            "SELECT Id, Time, NumScans, NumPeaks, TimsId, Polarity, ScanMode, MsMsType,
                    MzCalibration, AccumulationTime, SummedIntensities, MaxIntensity
             FROM Frames WHERE Id = ?1",
            [frame_id],
            frame_from_row,
        )?;
        Ok(frame)
    }

    /// All frames in ascending id order.
    pub fn frames(&self) -> Result<Vec<Frame>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let mut stmt = conn.prepare(
            "SELECT Id, Time, NumScans, NumPeaks, TimsId, Polarity, ScanMode, MsMsType,
                    MzCalibration, AccumulationTime, SummedIntensities, MaxIntensity
             FROM Frames ORDER BY Id ASC",
        )?;
        let rows = stmt.query_map([], frame_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    /// Return the diaPASEF isolation windows for an MS2 frame (SPEC §8.1).
    ///
    /// Returns `None` if the `DiaFrameMsMsInfo` table is absent (non-DIA bundle)
    /// or the frame has no entry (e.g. an MS1 frame).
    pub fn dia_windows_for_frame(&self, frame_id: u32) -> Result<Option<DiaFrameWindows>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='DiaFrameMsMsInfo'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            return Ok(None);
        }

        let window_group: Option<u32> = conn
            .query_row(
                "SELECT WindowGroup FROM DiaFrameMsMsInfo WHERE Frame = ?1",
                [frame_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(wg) = window_group else {
            return Ok(None);
        };

        let mut stmt = conn.prepare(
            "SELECT WindowGroup, ScanNumBegin, ScanNumEnd, IsolationMz, IsolationWidth, CollisionEnergy
             FROM DiaFrameMsMsWindows WHERE WindowGroup = ?1 ORDER BY ScanNumBegin ASC",
        )?;
        let windows: Vec<DiaWindow> = stmt
            .query_map([wg], |row| {
                Ok(DiaWindow {
                    window_group: row.get(0)?,
                    scan_num_begin: row.get(1)?,
                    scan_num_end: row.get(2)?,
                    isolation_mz: row.get(3)?,
                    isolation_width: row.get(4)?,
                    collision_energy: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        Ok(Some(DiaFrameWindows {
            frame_id,
            window_group: wg,
            windows,
        }))
    }

    /// Return the PASEF DDA MS2 scan ranges for a frame (SPEC §8.2).
    ///
    /// Returns an empty `Vec` if the `PasefFrameMsMsInfo` table is absent or
    /// this frame has no entries (e.g. an MS1 frame).
    pub fn pasef_msms_info_for_frame(&self, frame_id: u32) -> Result<Vec<PasefMsMsInfo>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='PasefFrameMsMsInfo'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT Frame, ScanNumBegin, ScanNumEnd, IsolationMz, IsolationWidth,
                    CollisionEnergy, Precursor
             FROM PasefFrameMsMsInfo WHERE Frame = ?1 ORDER BY ScanNumBegin ASC",
        )?;
        let rows: Vec<PasefMsMsInfo> = stmt
            .query_map([frame_id], |row| {
                Ok(PasefMsMsInfo {
                    frame_id: row.get(0)?,
                    scan_num_begin: row.get(1)?,
                    scan_num_end: row.get(2)?,
                    isolation_mz: row.get(3)?,
                    isolation_width: row.get(4)?,
                    collision_energy: row.get(5)?,
                    precursor_id: row.get::<_, i64>(6)? as u32,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Return the prm-PASEF MS2 scan ranges for a frame (SPEC §8.3).
    ///
    /// Returns an empty `Vec` if the `PrmFrameMsMsInfo` table is absent or
    /// this frame has no entries (e.g. an MS1 frame).
    pub fn prm_msms_info_for_frame(&self, frame_id: u32) -> Result<Vec<PrmMsMsInfo>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='PrmFrameMsMsInfo'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT Frame, ScanNumBegin, ScanNumEnd, IsolationMz, IsolationWidth,
                    CollisionEnergy, Target
             FROM PrmFrameMsMsInfo WHERE Frame = ?1 ORDER BY ScanNumBegin ASC",
        )?;
        let rows: Vec<PrmMsMsInfo> = stmt
            .query_map([frame_id], |row| {
                Ok(PrmMsMsInfo {
                    frame_id: row.get(0)?,
                    scan_num_begin: row.get(1)?,
                    scan_num_end: row.get(2)?,
                    isolation_mz: row.get(3)?,
                    isolation_width: row.get(4)?,
                    collision_energy: row.get(5)?,
                    target_id: row.get::<_, i64>(6)? as u32,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Look up a single PRM target by ID from the `PrmTargets` table (SPEC §8.3).
    ///
    /// Returns `None` if the table is absent or the target ID does not exist.
    pub fn prm_target(&self, target_id: u32) -> Result<Option<PrmTarget>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='PrmTargets'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            return Ok(None);
        }

        let result = conn
            .query_row(
                "SELECT Id, ExternalId, Time, OneOverK0, MonoisotopicMz, Charge, Description
                 FROM PrmTargets WHERE Id = ?1",
                [target_id],
                |row| {
                    Ok(PrmTarget {
                        id: row.get(0)?,
                        external_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        time: row.get(2)?,
                        one_over_k0: row.get(3)?,
                        monoisotopic_mz: row.get(4)?,
                        charge: row.get::<_, i64>(5)? as u32,
                        description: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// Look up a single precursor by ID from the `Precursors` table (SPEC §8.2).
    pub fn precursor(&self, precursor_id: u32) -> Result<Option<Precursor>> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='Precursors'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            return Ok(None);
        }

        let result = conn
            .query_row(
                "SELECT Id, LargestPeakMz, AverageMz, MonoisotopicMz, Charge,
                    ScanNumber, Intensity, Parent
             FROM Precursors WHERE Id = ?1",
                [precursor_id],
                |row| {
                    Ok(Precursor {
                        id: row.get(0)?,
                        largest_peak_mz: row.get(1)?,
                        average_mz: row.get(2)?,
                        monoisotopic_mz: row.get(3)?,
                        charge: row.get::<_, Option<i64>>(4)?.map(|v| v as u32),
                        scan_number: row.get(5)?,
                        intensity: row.get(6)?,
                        parent_frame_id: row.get::<_, i64>(7)? as u32,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// Decompress and decode the peaks of a single frame.
    ///
    /// Dispatches on `GlobalMetadata.TimsCompressionType`:
    /// * `2` → SPEC §4.4 (byte-transposed, delta-TOF over zstd).
    /// * `1` → SPEC §4.5 (per-scan LZF with signed-int32 delta stream).
    pub fn decode_peaks(&self, frame: &Frame) -> Result<Vec<Peak>> {
        match self.compression_type {
            2 => self.decode_peaks_codec2(frame),
            1 => self.decode_peaks_codec1(frame),
            other => Err(Error::UnsupportedCodec(other.to_string())),
        }
    }

    fn decode_peaks_codec2(&self, frame: &Frame) -> Result<Vec<Peak>> {
        let f = &self.tdf_bin;

        let mut header = [0u8; 8];
        read_at_exact(f, frame.tims_id, &mut header)?;
        let block_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let scan_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if scan_count != frame.num_scans {
            return Err(Error::CorruptFrame(
                frame.id,
                format!(
                    "header scan_count {} != Frames.NumScans {}",
                    scan_count, frame.num_scans
                ),
            ));
        }
        if block_size <= 8 {
            return Ok(Vec::new());
        }

        let payload_offset = frame.tims_id + 8;
        let file_len = self.tdf_bin_len;
        let payload_len = checked_block_len(file_len, payload_offset, u64::from(block_size) - 8)
            .ok_or_else(|| {
                Error::CorruptFrame(
                    frame.id,
                    format!(
                        "block payload (block_size {block_size}) at offset {payload_offset} \
                         exceeds remaining file size (file_len {file_len})"
                    ),
                )
            })?;
        let mut compressed = vec![0u8; payload_len];
        read_at_exact(f, payload_offset, &mut compressed)?;

        let expected_decompressed = 4 * (frame.num_scans as usize + 2 * frame.num_peaks as usize);
        let inner =
            zstd::bulk::decompress(&compressed, expected_decompressed).map_err(Error::Zstd)?;
        if inner.len() != expected_decompressed {
            return Err(Error::CorruptFrame(
                frame.id,
                format!(
                    "decompressed {} bytes, expected {}",
                    inner.len(),
                    expected_decompressed
                ),
            ));
        }

        Ok(decode_codec2(&inner, frame.num_scans, frame.num_peaks))
    }

    fn decode_peaks_codec1(&self, frame: &Frame) -> Result<Vec<Peak>> {
        let f = &self.tdf_bin;

        let mut header = [0u8; 8];
        read_at_exact(f, frame.tims_id, &mut header)?;
        let bin_size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let scan_count = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if scan_count != frame.num_scans {
            return Err(Error::CorruptFrame(
                frame.id,
                format!(
                    "header scan_count {} != Frames.NumScans {}",
                    scan_count, frame.num_scans
                ),
            ));
        }
        if bin_size <= 8 || frame.num_peaks == 0 {
            return Ok(Vec::new());
        }

        let file_len = self.tdf_bin_len;

        // u64 throughout: scan_count is read straight from the file, so
        // `(scan_count + 1) * 4` must not be allowed to overflow u32.
        let offsets_len = (u64::from(scan_count) + 1) * 4;
        let compression_offset = 8u64 + offsets_len;
        if u64::from(bin_size) < compression_offset {
            return Err(Error::CorruptFrame(
                frame.id,
                format!("bin_size {bin_size} < compression_offset {compression_offset}"),
            ));
        }

        let offsets_offset = frame.tims_id + 8;
        let offsets_table_len = checked_block_len(file_len, offsets_offset, offsets_len)
            .ok_or_else(|| {
                Error::CorruptFrame(
                    frame.id,
                    format!(
                        "scan offset table (scan_count {scan_count}) at offset {offsets_offset} \
                         exceeds remaining file size (file_len {file_len})"
                    ),
                )
            })?;
        let mut raw_offsets = vec![0u8; offsets_table_len];
        read_at_exact(f, offsets_offset, &mut raw_offsets)?;
        let mut scan_offsets = Vec::with_capacity(scan_count as usize + 1);
        let (chunks, _) = raw_offsets.as_chunks::<4>();
        for (i, chunk) in chunks.iter().enumerate() {
            let o = u64::from(u32::from_le_bytes(*chunk));
            // Offsets are relative to the frame start and must point past the
            // 8-byte header and the offset table itself. A smaller value means
            // the table is corrupt; rebasing it would silently decode the
            // wrong bytes.
            if o < compression_offset {
                return Err(Error::CorruptFrame(
                    frame.id,
                    format!("scan offset {i} = {o} < header size {compression_offset}"),
                ));
            }
            scan_offsets.push((o - compression_offset) as usize);
        }

        let compressed_offset = frame.tims_id + compression_offset;
        let compressed_len = checked_block_len(
            file_len,
            compressed_offset,
            u64::from(bin_size) - compression_offset,
        )
        .ok_or_else(|| {
            Error::CorruptFrame(
                frame.id,
                format!(
                    "compressed payload (bin_size {bin_size}) at offset {compressed_offset} \
                     exceeds remaining file size (file_len {file_len})"
                ),
            )
        })?;
        let mut compressed = vec![0u8; compressed_len];
        read_at_exact(f, compressed_offset, &mut compressed)?;

        decode_codec1(
            &compressed,
            &scan_offsets,
            frame.num_peaks,
            self.max_num_peaks_per_scan.max(1) as usize,
        )
        .map_err(|e| Error::CorruptFrame(frame.id, e))
    }
}

/// `GlobalMetadata.Value` for `key`, `None` when the key is absent.
fn global_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT Value FROM GlobalMetadata WHERE Key = ?1",
            [key],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

/// Like [`global_meta`] but a missing key is an error.
fn required_global_meta(conn: &Connection, key: &str) -> Result<String> {
    Ok(conn.query_row(
        "SELECT Value FROM GlobalMetadata WHERE Key = ?1",
        [key],
        |row| row.get::<_, String>(0),
    )?)
}

/// `(im_intercept, im_slope)` of the linear scan -> 1/K0 model.
fn mobility_calibration(conn: &Connection) -> Result<(f64, f64)> {
    let im_min: f64 = required_global_meta(conn, "OneOverK0AcqRangeLower")?
        .trim()
        .parse()
        .unwrap_or(0.0);
    let im_max: f64 = required_global_meta(conn, "OneOverK0AcqRangeUpper")?
        .trim()
        .parse()
        .unwrap_or(0.0);
    let scan_max: u32 = conn
        .query_row("SELECT MAX(NumScans) FROM Frames", [], |row| row.get(0))
        .unwrap_or(0);
    if im_min <= 0.0 || im_max <= im_min || scan_max == 0 {
        return Err(Error::CorruptFrame(
            0,
            format!(
                "invalid mobility calibration metadata: min={im_min} max={im_max} scan_max={scan_max}"
            ),
        ));
    }
    Ok((im_max, (im_min - im_max) / f64::from(scan_max)))
}

/// `(intercept, slope)` of the range-fallback model
/// `sqrt(mz) = intercept + slope * tof`.
fn range_mz_calibration(conn: &Connection) -> Result<(f64, f64)> {
    let mut mz_min: f64 = required_global_meta(conn, "MzAcqRangeLower")?
        .trim()
        .parse()
        .unwrap_or(0.0);
    let mut mz_max: f64 = required_global_meta(conn, "MzAcqRangeUpper")?
        .trim()
        .parse()
        .unwrap_or(0.0);
    let tof_max: u32 = required_global_meta(conn, "DigitizerNumSamples")?
        .trim()
        .parse()
        .unwrap_or(0);
    let acq_sw = global_meta(conn, "AcquisitionSoftware")?.unwrap_or_default();
    if acq_sw.trim() == "Bruker otofControl" {
        mz_min -= 5.0;
        mz_max += 5.0;
    }
    if mz_min <= 0.0 || mz_max <= mz_min || tof_max == 0 {
        return Err(Error::CorruptFrame(
            0,
            format!(
                "invalid m/z calibration metadata: min={mz_min} max={mz_max} tof_max={tof_max}"
            ),
        ));
    }
    let intercept = mz_min.sqrt();
    let slope = (mz_max.sqrt() - mz_min.sqrt()) / f64::from(tof_max);
    Ok((intercept, slope))
}

/// [`TablesMzModel`] for every `MzCalibration` row referenced by `Frames`,
/// plus the id of the row referenced by the most frames (lowest id on a
/// tie). `Err` carries the reason the tables cannot be used.
fn tables_mz_models(
    conn: &Connection,
    tof_max: Option<u32>,
) -> std::result::Result<(u32, BTreeMap<u32, TablesMzModel>), String> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='MzCalibration'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("cannot query the schema: {e}"))?;
    if exists == 0 {
        return Err("the MzCalibration table is absent".into());
    }
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT MzCalibration FROM Frames GROUP BY MzCalibration
             ORDER BY COUNT(*) DESC, MzCalibration ASC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>()
        })
        .map_err(|e| format!("cannot read Frames.MzCalibration: {e}"))?;
    let Some(&first) = ids.first() else {
        return Err("no frame references an MzCalibration row".into());
    };
    let to_id = |id: i64| {
        u32::try_from(id).map_err(|_| format!("Frames.MzCalibration value {id} is out of range"))
    };
    let primary = to_id(first)?;
    let mut rows = BTreeMap::new();
    for id in ids {
        let id = to_id(id)?;
        let model = mz_calibration_row(conn, id, tof_max)
            .map_err(|e| format!("MzCalibration row {id}: {e}"))?;
        rows.insert(id, model);
    }
    Ok((primary, rows))
}

/// Read and validate one `MzCalibration` row.
fn mz_calibration_row(
    conn: &Connection,
    id: u32,
    tof_max: Option<u32>,
) -> std::result::Result<TablesMzModel, String> {
    const COLUMNS: [&str; 8] = [
        "ModelType",
        "DigitizerTimebase",
        "DigitizerDelay",
        "C0",
        "C1",
        "C2",
        "C3",
        "C4",
    ];
    let values: Option<Vec<Value>> = conn
        .query_row(
            "SELECT ModelType, DigitizerTimebase, DigitizerDelay, C0, C1, C2, C3, C4
             FROM MzCalibration WHERE Id = ?1",
            [id],
            |row| (0..COLUMNS.len()).map(|i| row.get::<_, Value>(i)).collect(),
        )
        .optional()
        .map_err(|e| format!("cannot be read: {e}"))?;
    let Some(values) = values else {
        return Err("is referenced by Frames but missing from the table".into());
    };
    let mut v = [0.0f64; COLUMNS.len()];
    for (i, value) in values.iter().enumerate() {
        v[i] = match value {
            Value::Integer(n) => *n as f64,
            Value::Real(x) => *x,
            other => {
                return Err(format!(
                    "{} is not numeric ({:?})",
                    COLUMNS[i],
                    other.data_type()
                ))
            }
        };
    }
    let [model_type, digitizer_timebase, digitizer_delay, c0, c1, c2, c3, c4] = v;
    if model_type != 1.0 {
        return Err(format!(
            "ModelType {model_type} is not supported (only ModelType 1 is)"
        ));
    }
    if c3 != 0.0 {
        return Err(format!(
            "C3 = {c3} is non-zero and the C3 term has not been identified"
        ));
    }
    let model = TablesMzModel {
        digitizer_timebase,
        digitizer_delay,
        c0,
        c1,
        c2,
        c4,
    };
    model.validate(tof_max)?;
    Ok(model)
}
