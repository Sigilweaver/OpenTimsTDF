//! m/z calibration accuracy against ground truth stored in the bundle.
//!
//! For the most intense DDA precursors, the peak at `Precursors.LargestPeakMz`
//! is located in the parent MS1 frame (scans within +-12 of
//! `Precursors.ScanNumber`), its TOF centroid is computed from the decoded
//! peaks, and the centroid is converted with the bundle's calibration. The
//! test asserts on the median absolute ppm error between that m/z and
//! `LargestPeakMz`.
//!
//! Uses `common::test_bundle()` (honors `OPENTIMSTDF_TEST_BUNDLE` and
//! `REQUIRE_CORPUS`). Run with `--nocapture` to print the per-bundle figures
//! for both m/z models.

mod common;

use std::collections::HashMap;
use std::path::Path;

use openmassspec_core::SpectrumSource;
use opentimstdf::mzml::TdfSource;
use opentimstdf::{Calibration, MzCalibrationModel, Reader};
use rusqlite::{Connection, OpenFlags};

/// Precursors sampled, most intense first.
const MAX_PRECURSORS: usize = 2000;

/// Upper bound on the median |ppm| of the tables model.
///
/// Measured medians on public bundles: 0.90 ppm (PXD036417, the CI
/// bundle), 0.83 ppm (PXD027359 DDA), 0.86 ppm (PXD080079, timsTOF HT), and
/// 3.15 ppm on one third-party test fixture. A single TOF index of error
/// adds 4 to 9 ppm at 0.2 ns/index, and using the wrong `MzCalibration` row
/// or the range model adds tens to hundreds of ppm on most bundles, so 4 ppm
/// separates a working model from those failures while tolerating the
/// largest offset seen.
const MAX_MEDIAN_ABS_PPM: f64 = 4.0;

struct PrecursorRow {
    largest_mz: f64,
    scan: f64,
    parent: u32,
}

fn has_usable_mz_tables(bundle: &Path) -> bool {
    let conn = Connection::open_with_flags(
        bundle.join("analysis.tdf"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("open analysis.tdf");
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='MzCalibration'",
            [],
            |r| r.get(0),
        )
        .expect("schema query");
    if exists == 0 {
        return false;
    }
    // ModelType other than 1 is a known unsupported variant.
    let other_models: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM MzCalibration WHERE ModelType != 1 AND Id IN
             (SELECT DISTINCT MzCalibration FROM Frames)",
            [],
            |r| r.get(0),
        )
        .expect("ModelType query");
    other_models == 0
}

fn precursors(bundle: &Path) -> Vec<PrecursorRow> {
    let conn = Connection::open_with_flags(
        bundle.join("analysis.tdf"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("open analysis.tdf");
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='Precursors'",
            [],
            |r| r.get(0),
        )
        .expect("schema query");
    if exists == 0 {
        return Vec::new();
    }
    let mut stmt = conn
        .prepare(
            "SELECT LargestPeakMz, ScanNumber, Parent FROM Precursors
             WHERE Parent IS NOT NULL ORDER BY Intensity DESC LIMIT ?1",
        )
        .expect("prepare");
    let mut rows: Vec<PrecursorRow> = stmt
        .query_map([MAX_PRECURSORS as i64], |r| {
            Ok(PrecursorRow {
                largest_mz: r.get(0)?,
                scan: r.get(1)?,
                parent: r.get(2)?,
            })
        })
        .expect("query")
        .collect::<rusqlite::Result<_>>()
        .expect("rows");
    rows.sort_by_key(|p| p.parent);
    rows
}

/// Intensity-weighted TOF centroid (+-3 indices) of the most intense TOF
/// index within +-12 indices of `tof_guess`. `None` when the maximum sits
/// on the window edge or nothing is there.
fn centroid(profile: &HashMap<u32, f64>, tof_guess: f64) -> Option<f64> {
    const WINDOW: i64 = 12;
    const HALF: i64 = 3;
    let g = tof_guess.round() as i64;
    let mut best: Option<(i64, f64)> = None;
    for i in (g - WINDOW)..=(g + WINDOW) {
        let Ok(idx) = u32::try_from(i) else { continue };
        if let Some(&v) = profile.get(&idx) {
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((i, v));
            }
        }
    }
    let (imax, _) = best?;
    if (imax - g).abs() == WINDOW {
        return None;
    }
    let (mut weighted, mut total) = (0.0, 0.0);
    for i in (imax - HALF)..=(imax + HALF) {
        let Ok(idx) = u32::try_from(i) else { continue };
        if let Some(&v) = profile.get(&idx) {
            weighted += v * i as f64;
            total += v;
        }
    }
    (total > 0.0).then(|| weighted / total)
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn ppm(mz: f64, reference: f64) -> f64 {
    (mz - reference) / reference * 1e6
}

#[test]
fn mz_calibration_status_is_reported() {
    let Some(dir) = common::test_bundle() else {
        return;
    };
    let r = Reader::open(&dir).expect("open");
    let status = r.mz_calibration_status().expect("status");
    assert_eq!(
        status.model == MzCalibrationModel::RangeFallback,
        status.fallback_reason.is_some(),
        "fallback_reason must be set exactly for the range fallback: {status:?}"
    );
    if has_usable_mz_tables(&dir) {
        assert_eq!(
            status.model,
            MzCalibrationModel::Tables,
            "bundle has MzCalibration rows but uses the fallback: {:?}",
            status.fallback_reason
        );
    }
    assert_eq!(r.mz_calibration_model().expect("model"), status.model);
    assert_eq!(
        r.calibration().expect("calibration").mz_model(),
        status.model
    );

    let src = TdfSource::new(&r, "bundle.d").expect("source");
    let extra = src.run_metadata().extra;
    assert_eq!(
        extra.get("opentimstdf.mz_calibration").map(String::as_str),
        Some(status.model.as_str())
    );
    assert_eq!(
        extra.get("opentimstdf.mz_calibration_fallback_reason"),
        status.fallback_reason.as_ref()
    );
}

#[test]
fn mz_calibration_matches_precursor_peaks() {
    let Some(dir) = common::test_bundle() else {
        return;
    };
    let precs = precursors(&dir);
    if precs.is_empty() {
        eprintln!("skipping: {} has no DDA precursors", dir.display());
        return;
    }
    let r = Reader::open(&dir).expect("open");
    let bundle_cal = r.bundle_calibration().expect("calibration");
    if bundle_cal.status.model != MzCalibrationModel::Tables {
        eprintln!(
            "skipping: {} uses the range fallback ({:?})",
            dir.display(),
            bundle_cal.status.fallback_reason
        );
        return;
    }
    let fallback: Option<Calibration> = r.range_fallback_calibration().ok();

    let mut tables_err = Vec::new();
    let mut fallback_err = Vec::new();
    let mut current: Option<(u32, Vec<opentimstdf::Peak>, Calibration)> = None;
    for p in &precs {
        if current.as_ref().map(|c| c.0) != Some(p.parent) {
            let frame = r.frame(p.parent).expect("parent frame");
            let peaks = r.decode_peaks(&frame).expect("decode parent frame");
            current = Some((p.parent, peaks, *bundle_cal.for_frame(&frame)));
        }
        let Some((_, peaks, cal)) = current.as_ref() else {
            continue;
        };
        let lo = (p.scan - 12.0).max(0.0) as u32;
        let hi = (p.scan + 12.0) as u32;
        let mut profile: HashMap<u32, f64> = HashMap::new();
        for peak in peaks.iter().filter(|k| k.scan >= lo && k.scan <= hi) {
            *profile.entry(peak.tof).or_default() += f64::from(peak.intensity);
        }
        let Some(tof) = centroid(&profile, cal.mz_to_tof_f64(p.largest_mz)) else {
            continue;
        };
        tables_err.push(ppm(cal.tof_to_mz_f64(tof), p.largest_mz).abs());
        if let Some(f) = &fallback {
            fallback_err.push(ppm(f.tof_to_mz_f64(tof), p.largest_mz).abs());
        }
    }

    let n = tables_err.len();
    assert!(
        n >= 100 && n * 2 >= precs.len(),
        "matched only {n} of {} precursor peaks",
        precs.len()
    );
    let tables_median = median(&mut tables_err);
    let tables_p95 = percentile(&mut tables_err, 0.95);
    eprintln!(
        "{}: {n} precursors; tables |ppm| median {tables_median:.2} p95 {tables_p95:.2}",
        dir.display()
    );
    if !fallback_err.is_empty() {
        let fallback_median = median(&mut fallback_err);
        let fallback_p95 = percentile(&mut fallback_err, 0.95);
        eprintln!("  range fallback |ppm| median {fallback_median:.2} p95 {fallback_p95:.2}");
        assert!(
            tables_median < fallback_median,
            "tables model ({tables_median:.2} ppm) is not better than the range fallback \
             ({fallback_median:.2} ppm)"
        );
    }
    assert!(
        tables_median < MAX_MEDIAN_ABS_PPM,
        "median |ppm| {tables_median:.2} exceeds {MAX_MEDIAN_ABS_PPM}"
    );
}
