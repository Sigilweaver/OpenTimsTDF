//! mzML export for Bruker timsTOF `.d/` (TDF) bundles.
//!
//! Provides:
//!
//! * [`TdfSource`] - a [`openmassspec_core::SpectrumSource`] adapter over an
//!   open [`Reader`]. Use this when you want to feed timsTOF data into any
//!   `openmassspec-core`-shaped consumer (column store ingest, Arrow bridge,
//!   ...).
//! * [`write_mzml`] / [`write_indexed_mzml`] - convenience entry points
//!   that wrap the canonical writer in `openmassspec-core`.
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
//! skipped for now. `iter_spectra` decodes one frame at a time as the
//! iterator is driven; a frame that fails to decode is skipped rather than
//! aborting the whole run, per [`msc::SpectrumSource::iter_spectra`]'s
//! "skip silently" contract - the canonical writer trusts whatever the
//! iterator yields.

use std::io::Write;
use std::path::Path;

use openmassspec_core as msc;

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
        ("timsTOF SCP", "MS:1003231", "timsTOF SCP"),
        ("timsTOF HT", "MS:1003404", "timsTOF HT"),
        ("timsTOF Pro 2", "MS:1003230", "timsTOF Pro 2"),
        ("timsTOF Pro", "MS:1003005", "timsTOF Pro"),
        ("timsTOF fleX", "MS:1003124", "timsTOF fleX"),
        ("timsTOF", "MS:1003229", "timsTOF"),
        ("impact II", "MS:1002666", "impact II"),
        ("impact", "MS:1002077", "impact"),
        ("maXis II", "MS:1003004", "maXis II"),
        ("maXis", "MS:1001541", "maXis"),
    ];
    for (prefix, acc, term_name) in known {
        if name.starts_with(prefix) {
            return msc::CvTerm::new(acc, *term_name);
        }
    }
    msc::CvTerm::new("MS:1000122", "Bruker Daltonics instrument model")
}

/// Minimal structural check for `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)`.
///
/// Not a full RFC 3339 parser (no calendar/range validation) - just enough
/// to catch the one divergence that actually matters here: Bruker's
/// `AcquisitionDateTime` is documented as "ISO 8601", and ISO 8601 permits a
/// local time with no UTC offset at all, whereas RFC 3339 (what
/// [`msc::RunMetadata::start_timestamp`] promises) always requires one.
/// Every real `.d/` bundle inspected during this audit (schema versions
/// spanning multiple instruments/years) included a numeric offset, so this
/// is expected to pass in practice; the check exists so an unusual bundle
/// that omits the offset degrades to `None` instead of silently mislabeling
/// a local time as RFC 3339.
fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20 {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !(digits(0..4) && b[4] == b'-' && digits(5..7) && b[7] == b'-' && digits(8..10)) {
        return false;
    }
    if b[10] != b'T' && b[10] != b't' {
        return false;
    }
    if !(digits(11..13) && b[13] == b':' && digits(14..16) && b[16] == b':' && digits(17..19)) {
        return false;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let end = frac
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frac.len());
        if end == 0 {
            return false;
        }
        rest = &frac[end..];
    }
    if rest.eq_ignore_ascii_case("z") {
        return true;
    }
    let rb = rest.as_bytes();
    rb.len() == 6
        && (rb[0] == b'+' || rb[0] == b'-')
        && rb[1].is_ascii_digit()
        && rb[2].is_ascii_digit()
        && rb[3] == b':'
        && rb[4].is_ascii_digit()
        && rb[5].is_ascii_digit()
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
    inv_mobility_per_peak: Option<Vec<f32>>,
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
    let mut inv_mobility_per_peak = Vec::with_capacity(filtered.len());
    let mut tic: f64 = 0.0;
    let mut bp_mz = filtered[0].0;
    let mut bp_int: f32 = 0.0;
    let mut scan_sum: u64 = 0;
    for (m, i, s) in &filtered {
        mz.push(*m);
        intensity.push(*i);
        inv_mobility_per_peak.push(cal.scan_to_inv_mobility(*s) as f32);
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
        inv_mobility_per_peak: Some(inv_mobility_per_peak),
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
        inv_mobility_per_peak: None,
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
        inv_mobility_per_peak: pa.inv_mobility_per_peak,
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
        inv_mobility_per_peak: None,
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
        // Same native-ID format used for the MS1 spectrum (`build_ms1`,
        // above), so mzML `spectrumRef` lookups round-trip: the MS1 frame
        // this precursor was selected from is `parent_frame_id`, and every
        // MS1 spectrum has exactly one scan (`scan=1`).
        precursor_native_id: tdf_prec.map(|p| format!("frame={} scan=1", p.parent_frame_id)),
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
        inv_mobility_per_peak: pa.inv_mobility_per_peak,
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
        inv_mobility_per_peak: None,
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
        inv_mobility_per_peak: pa.inv_mobility_per_peak,
    }
}

/// `SpectrumSource` adapter over an open [`Reader`].
///
/// Construct with [`TdfSource::new`]; iterate via the trait's
/// [`iter_spectra`](openmassspec_core::SpectrumSource::iter_spectra) method.
///
/// `iter_spectra` decodes and projects one frame at a time as the returned
/// iterator is driven, so memory use is bounded by a single frame's worth
/// of spectra (PASEF/diaPASEF frames fan out into several spectra each,
/// which are queued and drained before the next frame is decoded) rather
/// than the whole run.
pub struct TdfSource<'a> {
    reader: &'a Reader,
    bundle_name: String,
    metadata: Metadata,
    calibration: Calibration,
    frames: Vec<Frame>,
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
        })
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
}

/// Project one frame into zero or more spectra, incrementing `scan_counter`
/// for each spectrum produced. Any decode failure - the frame's peaks, its
/// PASEF info rows, or its diaPASEF windows - causes that frame to be
/// skipped (returns an empty `Vec`) rather than aborting the caller's
/// iteration, matching [`msc::SpectrumSource::iter_spectra`]'s silent-skip
/// contract. A precursor lookup failure for a single PASEF MS2 is narrower:
/// it only drops that spectrum's precursor metadata, not the spectrum.
fn spectra_for_frame(
    reader: &Reader,
    frame: &Frame,
    calibration: &Calibration,
    scan_counter: &mut u32,
) -> Vec<msc::SpectrumRecord> {
    let Ok(peaks) = reader.decode_peaks(frame) else {
        return Vec::new();
    };
    match frame.msms_type {
        0 => {
            *scan_counter += 1;
            vec![build_ms1(*scan_counter, frame, &peaks, calibration)]
        }
        8 => {
            let Ok(infos) = reader.pasef_msms_info_for_frame(frame.id) else {
                return Vec::new();
            };
            let mut out = Vec::with_capacity(infos.len());
            for info in infos {
                let prec = reader.precursor(info.precursor_id).ok().flatten();
                *scan_counter += 1;
                out.push(build_pasef_ms2(
                    *scan_counter,
                    frame,
                    &info,
                    prec.as_ref(),
                    &peaks,
                    calibration,
                ));
            }
            out
        }
        9 => {
            let Ok(windows) = reader.dia_windows_for_frame(frame.id) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            if let Some(group) = windows {
                for w in &group.windows {
                    *scan_counter += 1;
                    out.push(build_dia_ms2(*scan_counter, frame, w, &peaks, calibration));
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Build a lazy, frame-at-a-time spectrum iterator. Decodes and projects
/// one frame per call to `next()` that yields nothing from the pending
/// queue, so at most one frame's peaks (plus its fan-out of PASEF/diaPASEF
/// spectra) are held in memory at a time.
fn frame_iter<'s>(
    reader: &'s Reader,
    frames: &'s [Frame],
    calibration: &'s Calibration,
) -> impl Iterator<Item = msc::SpectrumRecord> + 's {
    let mut frame_idx = 0usize;
    let mut pending = std::collections::VecDeque::new();
    let mut scan_counter: u32 = 0;
    std::iter::from_fn(move || loop {
        if let Some(rec) = pending.pop_front() {
            return Some(rec);
        }
        let frame = frames.get(frame_idx)?;
        frame_idx += 1;
        pending.extend(spectra_for_frame(
            reader,
            frame,
            calibration,
            &mut scan_counter,
        ));
    })
}

fn run_metadata_for(meta: &Metadata, bundle_name: &str) -> msc::RunMetadata {
    msc::RunMetadata {
        source_file_name: bundle_name.to_string(),
        source_file_format: source_file_format_cv(),
        native_id_format: native_id_format_cv(),
        instrument: instrument_cv(meta),
        software_name: SOFTWARE_NAME.into(),
        software_version: SOFTWARE_VERSION.into(),
        // `Metadata::acquisition_date_time` is read straight from the
        // `AcquisitionDateTime` row in `analysis.tdf`'s GlobalMetadata table
        // (see `Reader::metadata` in reader.rs) and documented there as an
        // ISO 8601 string. `RunMetadata::start_timestamp` promises RFC 3339,
        // which additionally requires a UTC offset; `is_rfc3339` guards
        // against a bundle whose value omits one rather than passing it
        // through unchecked.
        start_timestamp: meta
            .acquisition_date_time
            .as_deref()
            .filter(|s| is_rfc3339(s))
            .map(str::to_string),
        mobility_array_kind: Some(msc::MobilityArrayKind::InverseReducedVsPerCm2),
    }
}

impl<'a> msc::SpectrumSource for TdfSource<'a> {
    fn run_metadata(&self) -> msc::RunMetadata {
        run_metadata_for(&self.metadata, &self.bundle_name)
    }
    fn iter_spectra<'s>(&'s mut self) -> Box<dyn Iterator<Item = msc::SpectrumRecord> + 's> {
        Box::new(frame_iter(self.reader, &self.frames, &self.calibration))
    }
}

impl msc::SpectrumSource for OwnedTdfSource {
    fn run_metadata(&self) -> msc::RunMetadata {
        run_metadata_for(&self.metadata, &self.bundle_name)
    }
    fn iter_spectra<'s>(&'s mut self) -> Box<dyn Iterator<Item = msc::SpectrumRecord> + 's> {
        Box::new(frame_iter(&self.reader, &self.frames, &self.calibration))
    }
}

/// Write a `.d/` bundle to mzML in one call.
///
/// Convenience wrapper that opens `bundle_dir`, projects every frame into
/// the appropriate MS1 / PASEF MS2 / diaPASEF MS2 spectra, and emits a
/// valid mzML 1.1.0 document via the canonical writer in `openmassspec-core`.
pub fn write_mzml<P: AsRef<Path>, W: Write>(bundle_dir: P, out: &mut W) -> Result<()> {
    let mut src = TdfSource::open(bundle_dir)?;
    msc::write_mzml(&mut src, out)?;
    Ok(())
}

/// Indexed-mzML equivalent of [`write_mzml`].
pub fn write_indexed_mzml<P: AsRef<Path>, W: Write>(bundle_dir: P, out: &mut W) -> Result<()> {
    let mut src = TdfSource::open(bundle_dir)?;
    msc::write_indexed_mzml(&mut src, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_calibration() -> Calibration {
        Calibration {
            mz_intercept: 1.0,
            mz_slope: 0.001,
            im_intercept: 0.1,
            im_slope: 0.002,
        }
    }

    fn sample_frame(msms_type: u32) -> Frame {
        Frame {
            id: 7,
            time: 12.5,
            num_scans: 10,
            num_peaks: 3,
            tims_id: 0,
            scan_mode: msms_type,
            msms_type,
            mz_calibration_id: 1,
            accumulation_time: None,
            summed_intensities: None,
        }
    }

    #[test]
    fn materialize_peaks_wires_up_inv_mobility_per_peak() {
        let cal = sample_calibration();
        let peaks = [
            Peak {
                scan: 5,
                tof: 100,
                intensity: 10,
            },
            Peak {
                scan: 2,
                tof: 300,
                intensity: 20,
            },
            Peak {
                scan: 8,
                tof: 50,
                intensity: 5,
            },
        ];
        let pa = materialize_peaks(&peaks, &cal, None, None).expect("non-empty peaks");
        let inv_mobility_per_peak = pa
            .inv_mobility_per_peak
            .expect("peaks present -> Some per-peak array");

        // Parallel to `mz`/`intensity`: same length, and reordered by the
        // same mz-ascending sort (materialize_peaks sorts by mz, not scan).
        assert_eq!(inv_mobility_per_peak.len(), pa.mz.len());
        assert_eq!(inv_mobility_per_peak.len(), peaks.len());

        // mz ascending <=> tof ascending here (tof_to_mz is monotonic), so
        // the sorted order is tof 50, 100, 300 -> scans 8, 5, 2.
        let expected_scans = [8u32, 5, 2];
        for (got, &scan) in inv_mobility_per_peak.iter().zip(&expected_scans) {
            let want = cal.scan_to_inv_mobility(scan) as f32;
            assert_eq!(*got, want);
        }
    }

    #[test]
    fn materialize_peaks_none_when_no_peaks_in_range() {
        let cal = sample_calibration();
        let peaks = [Peak {
            scan: 1,
            tof: 100,
            intensity: 10,
        }];
        // scan_lo/scan_hi exclude the only peak.
        assert!(materialize_peaks(&peaks, &cal, Some(5), Some(10)).is_none());
    }

    #[test]
    fn build_pasef_ms2_wires_up_precursor_native_id_from_parent_frame() {
        let cal = sample_calibration();
        let frame = sample_frame(8);
        let peaks = [Peak {
            scan: 3,
            tof: 100,
            intensity: 10,
        }];
        let info = PasefMsMsInfo {
            frame_id: frame.id,
            scan_num_begin: 0,
            scan_num_end: 10,
            isolation_mz: 500.0,
            isolation_width: 2.0,
            collision_energy: 20.0,
            precursor_id: 1,
        };
        let prec = TdfPrecursor {
            id: 1,
            largest_peak_mz: 500.0,
            average_mz: 500.0,
            monoisotopic_mz: Some(500.0),
            charge: Some(2),
            scan_number: 3.0,
            intensity: 1000.0,
            parent_frame_id: 6,
        };

        let rec = build_pasef_ms2(1, &frame, &info, Some(&prec), &peaks, &cal);
        assert_eq!(
            rec.precursor
                .as_ref()
                .unwrap()
                .precursor_native_id
                .as_deref(),
            // Same format as build_ms1's native_id for frame 6, whose MS1
            // spectrum this precursor was selected from.
            Some("frame=6 scan=1")
        );
        assert_eq!(
            rec.inv_mobility_per_peak.as_ref().map(Vec::len),
            Some(peaks.len())
        );
    }

    #[test]
    fn build_pasef_ms2_precursor_native_id_none_without_precursor_row() {
        let cal = sample_calibration();
        let frame = sample_frame(8);
        let peaks = [Peak {
            scan: 3,
            tof: 100,
            intensity: 10,
        }];
        let info = PasefMsMsInfo {
            frame_id: frame.id,
            scan_num_begin: 0,
            scan_num_end: 10,
            isolation_mz: 500.0,
            isolation_width: 2.0,
            collision_energy: 20.0,
            precursor_id: 1,
        };

        let rec = build_pasef_ms2(1, &frame, &info, None, &peaks, &cal);
        assert_eq!(rec.precursor.as_ref().unwrap().precursor_native_id, None);
    }

    #[test]
    fn build_dia_ms2_leaves_precursor_native_id_none() {
        let cal = sample_calibration();
        let frame = sample_frame(9);
        let peaks = [Peak {
            scan: 3,
            tof: 100,
            intensity: 10,
        }];
        let window = DiaWindow {
            window_group: 1,
            scan_num_begin: 0,
            scan_num_end: 10,
            isolation_mz: 500.0,
            isolation_width: 2.0,
            collision_energy: 20.0,
        };

        let rec = build_dia_ms2(1, &frame, &window, &peaks, &cal);
        assert_eq!(rec.precursor.as_ref().unwrap().precursor_native_id, None);
        assert_eq!(
            rec.inv_mobility_per_peak.as_ref().map(Vec::len),
            Some(peaks.len())
        );
    }

    fn sample_metadata(acquisition_date_time: Option<&str>) -> Metadata {
        Metadata {
            schema_version_major: 3,
            schema_version_minor: 7,
            instrument_name: "timsTOF Pro".into(),
            acquisition_software: "timsControl".into(),
            acquisition_software_version: "2.0.18".into(),
            compression_type: 2,
            acquisition_date_time: acquisition_date_time.map(str::to_string),
        }
    }

    #[test]
    fn is_rfc3339_accepts_real_bruker_values() {
        // Pulled from GlobalMetadata.AcquisitionDateTime in real .d/ bundles
        // (multiple instruments/schema versions) - all include a numeric
        // UTC offset.
        for s in [
            "2018-08-21T20:40:14.356+02:00",
            "2022-02-22T23:47:02.147-08:00",
            "2026-03-30T22:54:48.394-07:00",
            "2019-08-24T18:27:02.345+02:00",
            "2026-01-01T00:00:00Z",
        ] {
            assert!(is_rfc3339(s), "expected {s} to be accepted");
        }
    }

    #[test]
    fn is_rfc3339_rejects_missing_offset_and_garbage() {
        for s in [
            "2019-01-17T09:14:39.730", // ISO 8601 local time, no offset
            "2019-01-17 09:14:39",     // space separator, no offset
            "not-a-timestamp",
            "",
        ] {
            assert!(!is_rfc3339(s), "expected {s:?} to be rejected");
        }
    }

    #[test]
    fn run_metadata_for_wires_up_start_timestamp() {
        let meta = sample_metadata(Some("2018-08-21T20:40:14.356+02:00"));
        let rm = run_metadata_for(&meta, "bundle.d");
        assert_eq!(
            rm.start_timestamp.as_deref(),
            Some("2018-08-21T20:40:14.356+02:00")
        );
    }

    #[test]
    fn run_metadata_for_none_when_absent() {
        let meta = sample_metadata(None);
        let rm = run_metadata_for(&meta, "bundle.d");
        assert_eq!(rm.start_timestamp, None);
    }

    #[test]
    fn run_metadata_for_none_when_not_rfc3339() {
        // Defensive path: don't claim RFC 3339 compliance for a value that
        // isn't, even though no real-world bundle observed so far hits this.
        let meta = sample_metadata(Some("2019-01-17T09:14:39.730"));
        let rm = run_metadata_for(&meta, "bundle.d");
        assert_eq!(rm.start_timestamp, None);
    }

    // Regression test: the lookup table in `instrument_cv` previously
    // carried several transcription errors against the real PSI-MS CV
    // (psi-ms.obo) - most seriously, "impact" resolved to MS:1001581,
    // which is actually "FAIMS compensation voltage", not a Bruker
    // instrument model. Every (name, accession) pair here was checked
    // against psi-ms.obo directly, not copied from the prior table.
    #[test]
    fn instrument_cv_resolves_known_models_to_correct_psi_ms_accessions() {
        let cases = [
            ("timsTOF SCP", "MS:1003231", "timsTOF SCP"),
            ("timsTOF HT", "MS:1003404", "timsTOF HT"),
            ("timsTOF Pro 2", "MS:1003230", "timsTOF Pro 2"),
            ("timsTOF Pro", "MS:1003005", "timsTOF Pro"),
            ("timsTOF fleX", "MS:1003124", "timsTOF fleX"),
            ("timsTOF", "MS:1003229", "timsTOF"),
            ("impact II", "MS:1002666", "impact II"),
            ("impact", "MS:1002077", "impact"),
            ("maXis II", "MS:1003004", "maXis II"),
            ("maXis", "MS:1001541", "maXis"),
            (
                "some future model nobody has heard of",
                "MS:1000122",
                "Bruker Daltonics instrument model",
            ),
        ];
        for (name, acc, term_name) in cases {
            let mut meta = sample_metadata(None);
            meta.instrument_name = name.into();
            let cv = instrument_cv(&meta);
            assert_eq!(cv.accession, acc, "wrong accession for {name:?}");
            assert_eq!(cv.name, term_name, "wrong CV name for {name:?}");
        }
    }
}
