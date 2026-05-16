//! End-to-end check: decode a known-good codec-2 frame and verify the
//! decoded intensity sum against the in-DB `Frames.SummedIntensities`.
//!
//! Guarded by an env var so CI without the corpus skips silently.

use std::path::PathBuf;

fn bundle_dir(rel: &str) -> Option<PathBuf> {
    // PRIDE bundles are extracted on demand to re/artifacts/cache/ by the
    // Python probe scripts.  That directory is gitignored scratch space.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let p = root.join("re/artifacts/cache").join(rel);
    if p.join("analysis.tdf").exists() && p.join("analysis.tdf_bin").exists() {
        Some(p)
    } else {
        None
    }
}

fn probe_dir(accession: &str) -> PathBuf {
    // Probe corpus files live in corpus/probes/<accession>/ and are always
    // present in the repository.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.join("corpus/probes").join(accession)
}

#[test]
fn pride_pxd027359_single_peak_frames_exact_match() {
    let Some(dir) = bundle_dir(
        "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d",
    ) else {
        eprintln!("skipping: PXD027359 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    assert_eq!(r.compression_type(), 2);

    let mut checked = 0;
    for frame in r.frames().expect("frames") {
        if frame.num_peaks == 1 {
            let peaks = r.decode_peaks(&frame).expect("decode");
            assert_eq!(peaks.len(), 1);
            let sum = u64::from(peaks[0].intensity);
            assert_eq!(Some(sum), frame.summed_intensities);
            checked += 1;
            if checked >= 10 {
                break;
            }
        }
    }
    assert!(checked > 0, "no single-peak frames found");
}

#[test]
fn calibration_ranges_match_metadata() {
    // At tof=0 the m/z should equal MzAcqRangeLower; at tof=DigitizerNumSamples
    // it should equal MzAcqRangeUpper. Similarly for 1/K₀: scan=0 → upper,
    // scan=scan_max → lower.
    let Some(dir) = bundle_dir(
        "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d",
    ) else {
        eprintln!("skipping: PXD027359 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    let c = r.calibration().expect("calibration");

    // Values pulled by hand from analysis.tdf GlobalMetadata:
    //   MzAcqRangeLower=100, MzAcqRangeUpper=1700, DigitizerNumSamples=394531
    //   OneOverK0AcqRangeLower=0.6, OneOverK0AcqRangeUpper=1.6
    //   MAX(NumScans)=927
    assert!((c.tof_to_mz(0) - 100.0).abs() < 1e-6);
    assert!((c.tof_to_mz(394_531) - 1700.0).abs() < 1e-3);
    assert!((c.scan_to_inv_mobility(0) - 1.6).abs() < 1e-9);
    assert!((c.scan_to_inv_mobility(927) - 0.6).abs() < 1e-9);

    // Round-trip one value through mz_to_tof and back.
    let tof0 = c.mz_to_tof(500.0);
    let mz_back = c.tof_to_mz(tof0);
    assert!(
        (mz_back - 500.0).abs() < 0.02,
        "mz roundtrip drift {mz_back}"
    );
}

#[test]
fn pride_pxd022216_codec1_numpeaks_match() {
    let bundle = match bundle_dir("pride/PXD022216/fmeierab_T190525_CLL_diaPASEF_02_1977.d") {
        Some(b) => b,
        None => {
            eprintln!("skipping: PXD022216 codec-1 bundle not present");
            return;
        }
    };
    let r = opentdf::Reader::open(&bundle).expect("open");
    assert_eq!(r.compression_type(), 1, "bundle should be codec 1");

    // Spot-check a handful of frames to avoid decoding the whole run.
    // DigitizerNumSamples is 394_531 for this bundle, but codec-1 frames
    // can emit occasional TOF values slightly beyond that (Bruker appears
    // to allow a small overflow window); use a generous sanity cap.
    let tof_sanity_cap: u32 = 1_000_000;
    for fid in [1u32, 2, 5, 100, 500] {
        let frame = match r.frame(fid) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let peaks = r.decode_peaks(&frame).expect("decode codec 1");
        assert_eq!(
            peaks.len() as u32,
            frame.num_peaks,
            "frame {fid}: decoded {} peaks, Frames.NumPeaks={}",
            peaks.len(),
            frame.num_peaks,
        );
        for p in &peaks {
            assert!(
                p.scan < frame.num_scans,
                "frame {fid}: scan {} >= NumScans {}",
                p.scan,
                frame.num_scans
            );
            assert!(
                p.tof < tof_sanity_cap,
                "frame {fid}: tof {} exceeds sanity cap {}",
                p.tof,
                tof_sanity_cap
            );
        }
    }
}

#[test]
fn pride_pxd039066_schema37_single_peak_frames() {
    let Some(dir) = bundle_dir("pride/PXD039066/TCell_10C_22G_I50_L25_Slot1-38_1_2723.d") else {
        eprintln!("skipping: PXD039066 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    assert_eq!(r.compression_type(), 2);

    let meta = r.metadata().expect("metadata");
    assert_eq!(meta.schema_version_major, 3);
    assert_eq!(meta.schema_version_minor, 7);

    // PXD039066 has no single-peak frames (min NumPeaks = 206).
    // Verify structure: decoded peak count and scan/tof bounds on a handful
    // of low-peak frames.
    let frames = r.frames().expect("frames");
    let mut checked = 0;
    for frame in frames.iter() {
        if frame.num_peaks > 0 && frame.num_peaks < 300 {
            let peaks = r.decode_peaks(frame).expect("decode");
            assert_eq!(
                peaks.len() as u32,
                frame.num_peaks,
                "frame {}: decoded {} peaks, NumPeaks={}",
                frame.id,
                peaks.len(),
                frame.num_peaks
            );
            for p in &peaks {
                assert!(
                    p.scan < frame.num_scans,
                    "frame {}: scan {} >= NumScans {}",
                    frame.id,
                    p.scan,
                    frame.num_scans
                );
            }
            checked += 1;
            if checked >= 5 {
                break;
            }
        }
    }
    assert!(checked > 0, "no suitable frames found for schema-3.7 check");
}

#[test]
fn frame_metadata_fields_populated() {
    // Frame now exposes time, scan_mode, msms_type, accumulation_time.
    // Verify on the codec-2 DDA bundle we always have available.
    let Some(dir) = bundle_dir(
        "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d",
    ) else {
        eprintln!("skipping: PXD027359 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    let frames = r.frames().expect("frames");
    assert!(!frames.is_empty());

    // MS1 frames have msms_type=0, scan_mode=8 (PASEF).
    let ms1: Vec<_> = frames.iter().filter(|f| f.msms_type == 0).collect();
    assert!(!ms1.is_empty(), "expected MS1 frames");
    for f in &ms1 {
        assert!(f.time > 0.0, "frame {} time should be positive", f.id);
        assert_eq!(f.mz_calibration_id, 1, "frame {} mz_calibration_id", f.id);
    }

    // MS2 frames have msms_type != 0.
    let ms2: Vec<_> = frames.iter().filter(|f| f.msms_type != 0).collect();
    assert!(!ms2.is_empty(), "expected MS2 frames");

    // Frames should be time-ordered (monotonically non-decreasing).
    for w in frames.windows(2) {
        assert!(
            w[1].time >= w[0].time,
            "frames not time-ordered: frame {} t={} before frame {} t={}",
            w[0].id,
            w[0].time,
            w[1].id,
            w[1].time
        );
    }
}

#[test]
fn pasef_msms_info_for_ms2_frame() {
    let Some(dir) = bundle_dir(
        "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d",
    ) else {
        eprintln!("skipping: PXD027359 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    let frames = r.frames().expect("frames");

    // Find the first MS2 frame (msms_type=8 for PASEF DDA).
    let ms2_frame = frames
        .iter()
        .find(|f| f.msms_type != 0)
        .expect("no MS2 frame");

    let entries = r
        .pasef_msms_info_for_frame(ms2_frame.id)
        .expect("pasef_msms_info");
    assert!(
        !entries.is_empty(),
        "expected PASEF MS2 entries for frame {}",
        ms2_frame.id
    );

    for entry in &entries {
        assert_eq!(entry.frame_id, ms2_frame.id);
        assert!(
            entry.scan_num_begin < entry.scan_num_end,
            "scan range invalid"
        );
        assert!(entry.isolation_mz > 0.0, "isolation_mz should be positive");
        assert!(
            entry.isolation_width > 0.0,
            "isolation_width should be positive"
        );
        assert!(
            entry.collision_energy > 0.0,
            "collision_energy should be positive"
        );

        // Precursor lookup should succeed.
        let prec = r.precursor(entry.precursor_id).expect("precursor query");
        assert!(prec.is_some(), "precursor {} not found", entry.precursor_id);
        let prec = prec.unwrap();
        assert!(prec.largest_peak_mz > 0.0);
        assert!(prec.parent_frame_id > 0);
    }
}

#[test]
fn dia_windows_for_ms2_frame() {
    // PXD025576 is our verified diaPASEF bundle.
    let Some(dir) = bundle_dir("pride/PXD025576/20210503_TIMS05_PS_SA_WholeProt_DIAMAX_100ng_1.d")
    else {
        eprintln!("skipping: PXD025576 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    let frames = r.frames().expect("frames");

    // MS1 frames should return None from dia_windows_for_frame.
    let ms1 = frames
        .iter()
        .find(|f| f.msms_type == 0)
        .expect("no MS1 frame");
    let none = r.dia_windows_for_frame(ms1.id).expect("dia query for MS1");
    assert!(none.is_none(), "MS1 frame should have no DIA windows");

    // MS2 frames (msms_type=9) should return windows.
    let ms2 = frames
        .iter()
        .find(|f| f.msms_type != 0)
        .expect("no MS2 frame");
    let fw = r
        .dia_windows_for_frame(ms2.id)
        .expect("dia query for MS2")
        .expect("expected DIA windows for MS2 frame");

    assert_eq!(fw.frame_id, ms2.id);
    assert!(!fw.windows.is_empty(), "window list should not be empty");
    for w in &fw.windows {
        assert_eq!(w.window_group, fw.window_group);
        assert!(w.scan_num_begin < w.scan_num_end);
        assert!(w.isolation_mz > 0.0);
        assert!(w.isolation_width > 0.0);
        assert!(w.collision_energy > 0.0);
    }
}

#[test]
fn pasef_bundle_has_no_dia_windows() {
    // A PASEF DDA bundle should return None from dia_windows_for_frame for all
    // frames, because DiaFrameMsMsInfo is empty (not absent, but no entries).
    let Some(dir) = bundle_dir(
        "pride/PXD027359/20201207_tims03_Evo03_PS_SA_HeLa_200ng_EvoSep_prot_DDA_21min_8cm_S1-C10_1_22476.d",
    ) else {
        eprintln!("skipping: PXD027359 cache not present");
        return;
    };
    let r = opentdf::Reader::open(&dir).expect("open");
    let frames = r.frames().expect("frames");

    let ms2 = frames
        .iter()
        .find(|f| f.msms_type != 0)
        .expect("no MS2 frame");
    let result = r.dia_windows_for_frame(ms2.id).expect("dia query");
    assert!(
        result.is_none(),
        "PASEF DDA MS2 frame should have no DIA windows"
    );
}

#[test]
fn prm_pasef_pxd028279_frame_distribution() {
    // Verify prm-PASEF frame metadata from PXD028279 (Brzhozovskiy et al. 2022).
    // Only analysis.tdf is present in the probe directory (no .tdf_bin); this
    // test covers the SQLite metadata path only.
    let dir = probe_dir("PXD028279");
    if !dir.join("analysis.tdf").exists() {
        eprintln!("skipping: probe corpus {} not present", dir.display());
        return;
    }
    let r = opentdf::Reader::open(&dir).expect("open PXD028279 PRM probe");

    let frames = r.frames().expect("frames");
    let total = frames.len();

    let ms1_count = frames.iter().filter(|f| f.msms_type == 0).count();
    let prm_count = frames.iter().filter(|f| f.msms_type == 10).count();

    assert_eq!(total, 36_741, "expected 36741 total frames");
    assert_eq!(ms1_count, 26_171, "expected 26171 MS1 frames");
    assert_eq!(prm_count, 10_570, "expected 10570 PRM frames (MsMsType=10)");

    // PRM frames should all have ScanMode=10.
    for f in frames.iter().filter(|f| f.msms_type == 10) {
        assert_eq!(
            f.scan_mode, 10,
            "PRM frame {} has unexpected ScanMode {}",
            f.id, f.scan_mode
        );
    }

    // DIA windows should be absent (diaPASEF and PRM are mutually exclusive).
    let prm_frame = frames.iter().find(|f| f.msms_type == 10).unwrap();
    let dia = r.dia_windows_for_frame(prm_frame.id).expect("dia query");
    assert!(dia.is_none(), "PRM frame should have no DIA windows");

    // PASEF MS/MS info table should be empty for PRM frames (it's a separate table).
    let pasef = r
        .pasef_msms_info_for_frame(prm_frame.id)
        .expect("pasef query");
    assert!(
        pasef.is_empty(),
        "PRM frame should have no PasefFrameMsMsInfo entries"
    );

    // prm_msms_info_for_frame should return entries for the first PRM frame.
    let prm_info = r
        .prm_msms_info_for_frame(prm_frame.id)
        .expect("prm_msms_info query");
    assert!(
        !prm_info.is_empty(),
        "expected PrmFrameMsMsInfo rows for frame {}",
        prm_frame.id
    );
    for entry in &prm_info {
        assert_eq!(entry.frame_id, prm_frame.id);
        assert!(entry.scan_num_begin < entry.scan_num_end);
        assert!(entry.isolation_mz > 0.0);
        assert!(
            (entry.isolation_width - 3.0).abs() < 0.01,
            "expected 3.0 Da isolation width"
        );
        assert!(entry.collision_energy > 0.0);
        assert!(entry.target_id > 0);

        // prm_target lookup should succeed.
        let target = r.prm_target(entry.target_id).expect("prm_target query");
        assert!(target.is_some(), "target {} not found", entry.target_id);
        let target = target.unwrap();
        assert!(target.monoisotopic_mz > 0.0);
        assert!(target.charge > 0);
        assert!(target.time > 0.0, "expected non-zero scheduled time");
    }

    // Calibration should be readable.
    let cal = r.calibration().expect("calibration");
    // sv=3.5 bundle: MzAcqRangeLower=100, MzAcqRangeUpper=1700, DigitizerNumSamples=393418
    assert!((cal.tof_to_mz(0) - 100.0).abs() < 1.0, "mz lower bound");
    assert!(
        (cal.tof_to_mz(393_418) - 1700.0).abs() < 5.0,
        "mz upper bound"
    );
}
