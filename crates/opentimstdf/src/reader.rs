use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};

use crate::calibration::Calibration;
use crate::codec::{decode_codec1, decode_codec2, frame_from_row};
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
        // so a missing row must default to 0 rather than fail Reader::open
        // (matches the old lazy per-call lookup's tolerance, just eagerly).
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

    /// Build the open-source calibration object for this bundle (SPEC §5 and §6).
    ///
    /// Uses `GlobalMetadata` acquisition-range values (`MzAcqRangeLower/Upper`,
    /// `DigitizerNumSamples`, `OneOverK0AcqRangeLower/Upper`) to construct the
    /// linear-in-sqrt(m/z) and linear 1/K0 approximation implemented by
    /// `opentims` (BSD-2-Clause). This model is the same for all frames,
    /// including dual-polarity bundles - per-polarity differentiation requires
    /// the proprietary Bruker polynomial model (SPEC §11 `[open]`).
    ///
    /// `frame.mz_calibration_id` identifies which polarity row a frame belongs
    /// to (1 = positive, 2 = negative in dual-polarity bundles) and is available
    /// for informational use, but does not affect the open-source calibration
    /// computation.
    pub fn calibration(&self) -> Result<Calibration> {
        fn meta(conn: &Connection, key: &str) -> Result<String> {
            Ok(conn.query_row(
                "SELECT Value FROM GlobalMetadata WHERE Key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )?)
        }

        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let mut mz_min: f64 = meta(&conn, "MzAcqRangeLower")?
            .trim()
            .parse()
            .unwrap_or(0.0);
        let mut mz_max: f64 = meta(&conn, "MzAcqRangeUpper")?
            .trim()
            .parse()
            .unwrap_or(0.0);
        let tof_max: u32 = meta(&conn, "DigitizerNumSamples")?
            .trim()
            .parse()
            .unwrap_or(0);
        let acq_sw = meta(&conn, "AcquisitionSoftware").unwrap_or_default();
        if acq_sw.trim() == "Bruker otofControl" {
            mz_min -= 5.0;
            mz_max += 5.0;
        }

        let im_min: f64 = meta(&conn, "OneOverK0AcqRangeLower")?
            .trim()
            .parse()
            .unwrap_or(0.0);
        let im_max: f64 = meta(&conn, "OneOverK0AcqRangeUpper")?
            .trim()
            .parse()
            .unwrap_or(0.0);
        let scan_max: u32 = conn
            .query_row("SELECT MAX(NumScans) FROM Frames", [], |row| row.get(0))
            .unwrap_or(0);

        if mz_min <= 0.0 || mz_max <= mz_min || tof_max == 0 {
            return Err(Error::CorruptFrame(
                0,
                format!(
                    "invalid m/z calibration metadata: min={mz_min} max={mz_max} tof_max={tof_max}"
                ),
            ));
        }
        if im_min <= 0.0 || im_max <= im_min || scan_max == 0 {
            return Err(Error::CorruptFrame(
                0,
                format!(
                    "invalid mobility calibration metadata: min={im_min} max={im_max} scan_max={scan_max}"
                ),
            ));
        }

        let mz_intercept = mz_min.sqrt();
        let mz_slope = (mz_max.sqrt() - mz_min.sqrt()) / f64::from(tof_max);
        let im_intercept = im_max;
        let im_slope = (im_min - im_max) / f64::from(scan_max);

        Ok(Calibration {
            mz_intercept,
            mz_slope,
            im_intercept,
            im_slope,
        })
    }

    pub fn frame(&self, frame_id: u32) -> Result<Frame> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        let frame = conn.query_row(
            "SELECT Id, Time, NumScans, NumPeaks, TimsId, ScanMode, MsMsType,
                    MzCalibration, AccumulationTime, SummedIntensities
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
            "SELECT Id, Time, NumScans, NumPeaks, TimsId, ScanMode, MsMsType,
                    MzCalibration, AccumulationTime, SummedIntensities
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
        if block_size == 8 {
            return Ok(Vec::new());
        }

        let payload_len = (block_size - 8) as usize;
        let mut compressed = vec![0u8; payload_len];
        read_at_exact(f, frame.tims_id + 8, &mut compressed)?;

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
        if bin_size == 8 || frame.num_peaks == 0 {
            return Ok(Vec::new());
        }

        let compression_offset = 8u32 + (scan_count + 1) * 4;
        if bin_size < compression_offset {
            return Err(Error::CorruptFrame(
                frame.id,
                format!("bin_size {bin_size} < compression_offset {compression_offset}"),
            ));
        }

        let mut raw_offsets = vec![0u8; ((scan_count + 1) * 4) as usize];
        read_at_exact(f, frame.tims_id + 8, &mut raw_offsets)?;
        let mut scan_offsets = Vec::with_capacity(scan_count as usize + 1);
        for chunk in raw_offsets.chunks_exact(4) {
            // chunks_exact(4) guarantees chunk.len() == 4
            #[allow(clippy::unwrap_used)]
            let o = u32::from_le_bytes(chunk.try_into().unwrap());
            scan_offsets.push(o.saturating_sub(compression_offset) as usize);
        }

        let compressed_len = (bin_size - compression_offset) as usize;
        let mut compressed = vec![0u8; compressed_len];
        read_at_exact(
            f,
            frame.tims_id + u64::from(compression_offset),
            &mut compressed,
        )?;

        decode_codec1(
            &compressed,
            &scan_offsets,
            frame.num_peaks,
            self.max_num_peaks_per_scan.max(1) as usize,
        )
        .map_err(|e| Error::CorruptFrame(frame.id, e))
    }
}
