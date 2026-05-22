//! mzML export for Bruker timsTOF `.d/` (TDF) bundles.
//!
//! Provides:
//!
//! * [`TdfSource`] - a [`openproteo_core::SpectrumSource`] adapter over an
//!   open [`Reader`]. Use this when you want to feed timsTOF data into any
//!   `openproteo-core`-shaped consumer (column store ingest, Arrow bridge,
//!   ...).
//! * [`write_mzml`] / [`write_indexed_mzml`] - convenience entry points
//!   that wrap the canonical writer in `openproteo-core`.
//!
//! Frame -> spectrum projection:
//!
//! * **MS1 frames** (`msms_type == 0`): all mobility scans pooled into one
//!   MS1 spectrum, sorted by m/z. Native ID: `frame=F scan=1`.
//! * **PASEF DDA frames** (`msms_type == 8`): one MS2 spectrum per
//!   `PasefMsMsInfo` row, filtered to that row's mobility scan range, with
//!   precursor m/z / charge / intensity copied from the `Precursors` table.
//!   Native ID: `frame=F scan=A-B`.
//! * **diaPASEF frames** (`msms_type == 9`): one MS2 spectrum per
//!   `DiaWindow` in the frame, with the isolation window taken from the
//!   `DiaFrameMsMsWindows` table. Native ID: `frame=F scan=A-B`.
//!
//! `scan_number` is a monotonic 1-based counter across the whole bundle
//! (PASEF frames produce many spectra per frame, so frame ID alone is not
//! unique).
//!
//! Frames with `msms_type` other than 0/8/9 (e.g. PRM-PASEF = 10) are
//! skipped for now. Decode errors on individual frames bubble up to abort
//! the iteration; the canonical writer trusts whatever the iterator
//! yields.

use std::io::Write;
use std::path::Path;

use openproteo_core as msc;

use crate::error::Result;
use crate::{
    Calibration, DiaWindow, Frame, Metadata, PasefMsMsInfo, Peak, Precursor as TdfPrecursor, Reader,
};

const SOFTWARE_NAME: &str = "opentimstdf";
const SOFTWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

fn source_file_format_cv() -> msc::CvTerm {
    // PSI-MS MS:1002817 = "Bruker TDF format".
    msc::CvTerm::new("MS:1002817", "Bruker TDF format")
}

fn native_id_format_cv() -> msc::CvTerm {
    // PSI-MS MS:1002818 = "Bruker TDF nativeID format".
    msc::CvTerm::new("MS:1002818", "Bruker TDF nativeID format")
}

/// Resolve a PSI-MS instrument CV term from the GlobalMetadata
/// `InstrumentName`. Falls back to the generic Bruker term when the model
/// string is not in the lookup table.
fn instrument_cv(meta: &Metadata) -> msc::CvTerm {
    let name = meta.instrument_name.as_str();
    let known: &[(&str, &str, &str)] = &[
        ("timsTOF SCP", "MS:1003229", "timsTOF SCP"),
        ("timsTOF HT", "MS:1003404", "timsTOF HT"),
        ("timsTOF Pro 2", "MS:1003230", "timsTOF Pro 2"),
        ("timsTOF Pro", "MS:1003005", "timsTOF Pro"),
        ("timsTOF fleX", "MS:1003124", "timsTOF fleX"),
        ("timsTOF", "MS:1003005", "timsTOF"),
        ("impact II", "MS:1002280", "impact II"),
        ("impact", "MS:1001581", "Bruker Daltonics impact series"),
        ("maXis II", "MS:1002281", "maXis II"),
        ("maXis", "MS:1001541", "Bruker Daltonics maXis series"),
    ];
    for (prefix, acc, term_name) in known {
        if name.starts_with(prefix) {
            return msc::CvTerm::new(acc, *term_name);
        }
    }
    msc::CvTerm::new("MS:1000122", "Bruker Daltonics instrument model")
}

fn polarity_for(frame: &Frame) -> Option<msc::Polarity> {
    match frame.mz_calibration_id {
        1 => Some(msc::Polarity::Positive),
        2 => Some(msc::Polarity::Negative),
        _ => None,
    }
}

/// Project a slice of decoded peaks into the column arrays a
/// [`msc::SpectrumRecord`] expects, optionally filtered by mobility scan
/// range `[scan_lo, scan_hi)` (used for PASEF / diaPASEF sub-windows).
struct PeakArrays {
    mz: Vec<f64>,
    intensity: Vec<f32>,
    tic: f64,
    base_peak_mz: f64,
    base_peak_intensity: f64,
    low_mz: f64,
    high_mz: f64,
    inv_mobility: Option<f64>,
}

fn materialize_peaks(
    peaks: &[Peak],
    cal: &Calibration,
    scan_lo: Option<u32>,
    scan_hi: Option<u32>,
) -> Option<PeakArrays> {
    let mut filtered: Vec<(f64, f32, u32)> = Vec::new();
    for p in peaks {
        if let Some(lo) = scan_lo {
            if p.scan < lo {
                continue;
            }
        }
        if let Some(hi) = scan_hi {
            if p.scan >= hi {
                continue;
            }
        }
        let mz = cal.tof_to_mz(p.tof);
        filtered.push((mz, p.intensity as f32, p.scan));
    }
    if filtered.is_empty() {
        return None;
    }
    filtered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut mz = Vec::with_capacity(filtered.len());
    let mut intensity = Vec::with_capacity(filtered.len());
    let mut tic: f64 = 0.0;
    let mut bp_mz = filtered[0].0;
    let mut bp_int: f32 = 0.0;
    let mut scan_sum: u64 = 0;
    for (m, i, s) in &filtered {
        mz.push(*m);
        intensity.push(*i);
        tic += *i as f64;
        if *i > bp_int {
            bp_int = *i;
            bp_mz = *m;
        }
        scan_sum += *s as u64;
    }
    let mean_scan = scan_sum as f64 / filtered.len() as f64;
    let inv_mob = cal.scan_to_inv_mobility(mean_scan.round() as u32);

    let low_mz = filtered.first().map(|t| t.0).unwrap_or(0.0);
    let high_mz = filtered.last().map(|t| t.0).unwrap_or(0.0);

    Some(PeakArrays {
        mz,
        intensity,
        tic,
        base_peak_mz: bp_mz,
        base_peak_intensity: bp_int as f64,
        low_mz,
        high_mz,
        inv_mobility: Some(inv_mob),
    })
}

fn build_ms1(
    scan_number: u32,
    frame: &Frame,
    peaks: &[Peak],
    cal: &Calibration,
) -> msc::SpectrumRecord {
    let pa = materialize_peaks(peaks, cal, None, None).unwrap_or(PeakArrays {
        mz: Vec::new(),
        intensity: Vec::new(),
        tic: 0.0,
        base_peak_mz: 0.0,
        base_peak_intensity: 0.0,
        low_mz: 0.0,
        high_mz: 0.0,
        inv_mobility: None,
    });
    msc::SpectrumRecord {
        index: (scan_number as usize).saturating_sub(1),
        scan_number,
        native_id: format!("frame={} scan=1", frame.id),
        ms_level: 1,
        polarity: polarity_for(frame),
        scan_mode: Some(msc::ScanMode::Centroid),
        analyzer: Some(msc::Analyzer::TOFMS),
        filter: None,
        retention_time_sec: frame.time,
        total_ion_current: Some(pa.tic),
        base_peak_mz: Some(pa.base_peak_mz),
        base_peak_intensity: Some(pa.base_peak_intensity),
        low_mz: Some(pa.low_mz),
        high_mz: Some(pa.high_mz),
        ion_injection_time_ms: frame.accumulation_time,
        inv_mobility: pa.inv_mobility,
        precursor: None,
        mz: pa.mz,
        intensity: pa.intensity,
        inv_mobility_per_peak: None,
    }
}

fn build_pasef_ms2(
    scan_number: u32,
    frame: &Frame,
    info: &PasefMsMsInfo,
    tdf_prec: Option<&TdfPrecursor>,
    peaks: &[Peak],
    cal: &Calibration,
) -> msc::SpectrumRecord {
    let pa = materialize_peaks(
        peaks,
        cal,
        Some(info.scan_num_begin),
        Some(info.scan_num_end),
    )
    .unwrap_or(PeakArrays {
        mz: Vec::new(),
        intensity: Vec::new(),
        tic: 0.0,
        base_peak_mz: 0.0,
        base_peak_intensity: 0.0,
        low_mz: 0.0,
        high_mz: 0.0,
        inv_mobility: None,
    });
    let prec_mz = tdf_prec
        .and_then(|p| p.monoisotopic_mz)
        .or_else(|| tdf_prec.map(|p| p.largest_peak_mz));
    let precursor = Some(msc::PrecursorInfo {
        target_mz: Some(info.isolation_mz),
        selected_mz: prec_mz,
        isolation_width: Some(info.isolation_width),
        charge: tdf_prec.and_then(|p| p.charge).map(|c| c as i32),
        intensity: tdf_prec.map(|p| p.intensity),
        collision_energy: Some(info.collision_energy),
        ce_is_nce: false,
        precursor_native_id: None,
        activation: Some(msc::Activation::HCD),
        analyzer: Some(msc::Analyzer::TOFMS),
    });
    msc::SpectrumRecord {
        index: (scan_number as usize).saturating_sub(1),
        scan_number,
        native_id: format!(
            "frame={} scan={}-{}",
            frame.id, info.scan_num_begin, info.scan_num_end
        ),
        ms_level: 2,
        polarity: polarity_for(frame),
        scan_mode: Some(msc::ScanMode::Centroid),
        analyzer: Some(msc::Analyzer::TOFMS),
        filter: None,
        retention_time_sec: frame.time,
        total_ion_current: Some(pa.tic),
        base_peak_mz: Some(pa.base_peak_mz),
        base_peak_intensity: Some(pa.base_peak_intensity),
        low_mz: Some(pa.low_mz),
        high_mz: Some(pa.high_mz),
        ion_injection_time_ms: frame.accumulation_time,
        inv_mobility: pa.inv_mobility,
        precursor,
        mz: pa.mz,
        intensity: pa.intensity,
        inv_mobility_per_peak: None,
    }
}

fn build_dia_ms2(
    scan_number: u32,
    frame: &Frame,
    window: &DiaWindow,
    peaks: &[Peak],
    cal: &Calibration,
) -> msc::SpectrumRecord {
    let pa = materialize_peaks(
        peaks,
        cal,
        Some(window.scan_num_begin),
        Some(window.scan_num_end),
    )
    .unwrap_or(PeakArrays {
        mz: Vec::new(),
        intensity: Vec::new(),
        tic: 0.0,
        base_peak_mz: 0.0,
        base_peak_intensity: 0.0,
        low_mz: 0.0,
        high_mz: 0.0,
        inv_mobility: None,
    });
    let precursor = Some(msc::PrecursorInfo {
        target_mz: Some(window.isolation_mz),
        selected_mz: Some(window.isolation_mz),
        isolation_width: Some(window.isolation_width),
        charge: None,
        intensity: None,
        collision_energy: Some(window.collision_energy),
        ce_is_nce: false,
        precursor_native_id: None,
        activation: Some(msc::Activation::HCD),
        analyzer: Some(msc::Analyzer::TOFMS),
    });
    msc::SpectrumRecord {
        index: (scan_number as usize).saturating_sub(1),
        scan_number,
        native_id: format!(
            "frame={} scan={}-{}",
            frame.id, window.scan_num_begin, window.scan_num_end
        ),
        ms_level: 2,
        polarity: polarity_for(frame),
        scan_mode: Some(msc::ScanMode::Centroid),
        analyzer: Some(msc::Analyzer::TOFMS),
        filter: None,
        retention_time_sec: frame.time,
        total_ion_current: Some(pa.tic),
        base_peak_mz: Some(pa.base_peak_mz),
        base_peak_intensity: Some(pa.base_peak_intensity),
        low_mz: Some(pa.low_mz),
        high_mz: Some(pa.high_mz),
        ion_injection_time_ms: frame.accumulation_time,
        inv_mobility: pa.inv_mobility,
        precursor,
        mz: pa.mz,
        intensity: pa.intensity,
        inv_mobility_per_peak: None,
    }
}

/// `SpectrumSource` adapter over an open [`Reader`].
///
/// Construct with [`TdfSource::new`]; iterate via the trait's
/// [`iter_spectra`](openproteo_core::SpectrumSource::iter_spectra) method.
///
/// The iterator buffers every spectrum in memory up front. timsTOF data is
/// typically O(10 GB) raw / O(GB) decoded; for very large runs the caller
/// should iterate frame-by-frame using the lower-level [`Reader`] API and
/// build their own [`openproteo_core::SpectrumSource`].
pub struct TdfSource<'a> {
    reader: &'a Reader,
    bundle_name: String,
    metadata: Metadata,
    calibration: Calibration,
    frames: Vec<Frame>,
    /// Pre-built spectra; populated lazily on the first `iter_spectra` call.
    spectra: Option<Vec<msc::SpectrumRecord>>,
}

impl<'a> TdfSource<'a> {
    /// Build a new source from an open [`Reader`] and the bundle's directory
    /// name (used as the `<sourceFile name="...">` in mzML output).
    pub fn new(reader: &'a Reader, bundle_name: impl Into<String>) -> Result<Self> {
        let metadata = reader.metadata()?;
        let calibration = reader.calibration()?;
        let frames = reader.frames()?;
        Ok(Self {
            reader,
            bundle_name: bundle_name.into(),
            metadata,
            calibration,
            frames,
            spectra: None,
        })
    }

    /// Open a bundle directory and build a source in one call.
    ///
    /// The reader is owned by the returned `OwnedTdfSource`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<OwnedTdfSource> {
        let path = path.as_ref();
        let reader = Reader::open(path)?;
        let metadata = reader.metadata()?;
        let calibration = reader.calibration()?;
        let frames = reader.frames()?;
        let bundle_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bundle.d".into());
        Ok(OwnedTdfSource {
            reader,
            bundle_name,
            metadata,
            calibration,
            frames,
            spectra: None,
        })
    }

    fn build_spectra(&mut self) -> Result<&Vec<msc::SpectrumRecord>> {
        if self.spectra.is_none() {
            self.spectra = Some(project_frames(
                self.reader,
                &self.frames,
                &self.calibration,
            )?);
        }
        Ok(self.spectra.as_ref().unwrap())
    }
}

/// Self-contained variant of [`TdfSource`] that owns its [`Reader`]. Used
/// for the common case where the caller just wants `open() -> write_mzml`.
pub struct OwnedTdfSource {
    reader: Reader,
    bundle_name: String,
    metadata: Metadata,
    calibration: Calibration,
    frames: Vec<Frame>,
    spectra: Option<Vec<msc::SpectrumRecord>>,
}

impl OwnedTdfSource {
    fn build_spectra(&mut self) -> Result<&Vec<msc::SpectrumRecord>> {
        if self.spectra.is_none() {
            self.spectra = Some(project_frames(
                &self.reader,
                &self.frames,
                &self.calibration,
            )?);
        }
        Ok(self.spectra.as_ref().unwrap())
    }
}

fn project_frames(
    reader: &Reader,
    frames: &[Frame],
    calibration: &Calibration,
) -> Result<Vec<msc::SpectrumRecord>> {
    let mut spectra: Vec<msc::SpectrumRecord> = Vec::with_capacity(frames.len());
    let mut scan_counter: u32 = 0;
    for frame in frames {
        let peaks = reader.decode_peaks(frame)?;
        match frame.msms_type {
            0 => {
                scan_counter += 1;
                spectra.push(build_ms1(scan_counter, frame, &peaks, calibration));
            }
            8 => {
                let infos = reader.pasef_msms_info_for_frame(frame.id)?;
                for info in infos {
                    let prec = reader.precursor(info.precursor_id)?;
                    scan_counter += 1;
                    spectra.push(build_pasef_ms2(
                        scan_counter,
                        frame,
                        &info,
                        prec.as_ref(),
                        &peaks,
                        calibration,
                    ));
                }
            }
            9 => {
                let windows = reader.dia_windows_for_frame(frame.id)?;
                if let Some(group) = windows {
                    for w in &group.windows {
                        scan_counter += 1;
                        spectra.push(build_dia_ms2(scan_counter, frame, w, &peaks, calibration));
                    }
                }
            }
            _ => continue,
        }
    }
    Ok(spectra)
}

fn run_metadata_for(meta: &Metadata, bundle_name: &str, n_spectra: usize) -> msc::RunMetadata {
    let _ = n_spectra;
    msc::RunMetadata {
        source_file_name: bundle_name.to_string(),
        source_file_format: source_file_format_cv(),
        native_id_format: native_id_format_cv(),
        instrument: instrument_cv(meta),
        software_name: SOFTWARE_NAME.into(),
        software_version: SOFTWARE_VERSION.into(),
        start_timestamp: None,
        mobility_array_kind: Some(msc::MobilityArrayKind::InverseReducedVsPerCm2),
    }
}

impl<'a> msc::SpectrumSource for TdfSource<'a> {
    fn run_metadata(&self) -> msc::RunMetadata {
        let n = self.spectra.as_ref().map(|v| v.len()).unwrap_or(0);
        run_metadata_for(&self.metadata, &self.bundle_name, n)
    }
    fn iter_spectra<'s>(&'s mut self) -> Box<dyn Iterator<Item = msc::SpectrumRecord> + 's> {
        let recs = self.build_spectra().cloned().unwrap_or_default();
        Box::new(recs.into_iter())
    }
    fn spectrum_count_hint(&self) -> Option<usize> {
        self.spectra.as_ref().map(|v| v.len())
    }
}

impl msc::SpectrumSource for OwnedTdfSource {
    fn run_metadata(&self) -> msc::RunMetadata {
        let n = self.spectra.as_ref().map(|v| v.len()).unwrap_or(0);
        run_metadata_for(&self.metadata, &self.bundle_name, n)
    }
    fn iter_spectra<'s>(&'s mut self) -> Box<dyn Iterator<Item = msc::SpectrumRecord> + 's> {
        let recs = self.build_spectra().cloned().unwrap_or_default();
        Box::new(recs.into_iter())
    }
    fn spectrum_count_hint(&self) -> Option<usize> {
        self.spectra.as_ref().map(|v| v.len())
    }
}

/// Write a `.d/` bundle to mzML in one call.
///
/// Convenience wrapper that opens `bundle_dir`, projects every frame into
/// the appropriate MS1 / PASEF MS2 / diaPASEF MS2 spectra, and emits a
/// valid mzML 1.1.0 document via the canonical writer in `openproteo-core`.
pub fn write_mzml<P: AsRef<Path>, W: Write>(bundle_dir: P, out: &mut W) -> Result<()> {
    let mut src = TdfSource::open(bundle_dir)?;
    // Eagerly build so spectrum_count_hint is populated for the writer.
    src.build_spectra()?;
    msc::write_mzml(&mut src, out)?;
    Ok(())
}

/// Indexed-mzML equivalent of [`write_mzml`].
pub fn write_indexed_mzml<P: AsRef<Path>, W: Write>(bundle_dir: P, out: &mut W) -> Result<()> {
    let mut src = TdfSource::open(bundle_dir)?;
    src.build_spectra()?;
    msc::write_indexed_mzml(&mut src, out)?;
    Ok(())
}
