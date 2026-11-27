//! Frames the projection leaves out are counted and flagged, not dropped
//! silently (Sigilweaver/OpenTimsTDF#13), and the stated m/z acquisition
//! range becomes the scan window.
//!
//! Builds synthetic `analysis.tdf` files in the Cargo target tmp dir: only the
//! SQLite tables are read, so the `analysis.tdf_bin` is empty and no frame is
//! decoded.

use std::fs;
use std::path::PathBuf;

use openmassspec_core::SpectrumSource;
use opentimstdf::mzml::{TdfSource, FRAMES_NOT_PROJECTED_CODE, PRM_CHROMATOGRAM_GAPS_CODE};

/// One frame per `msms_type`, ids from 1.
fn bundle(name: &str, msms_types: &[u32], mz_range: Option<(&str, &str)>) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.d"));
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(dir.join("analysis.tdf_bin"), []).expect("tdf_bin");
    let db = dir.join("analysis.tdf");
    let _ = fs::remove_file(&db);
    let conn = rusqlite::Connection::open(&db).expect("sqlite");
    conn.execute_batch(
        "CREATE TABLE GlobalMetadata (Key TEXT PRIMARY KEY, Value TEXT);
         INSERT INTO GlobalMetadata VALUES
           ('TimsCompressionType', '2'),
           ('OneOverK0AcqRangeLower', '0.6'),
           ('OneOverK0AcqRangeUpper', '1.6'),
           ('MzAcqRangeLower', '100'),
           ('MzAcqRangeUpper', '1700'),
           ('DigitizerNumSamples', '400000'),
           ('InstrumentName', 'timsTOF Pro');
         CREATE TABLE Frames (Id INTEGER PRIMARY KEY, Time REAL, NumScans INTEGER,
           NumPeaks INTEGER, TimsId INTEGER, Polarity TEXT, ScanMode INTEGER,
           MsMsType INTEGER, MzCalibration INTEGER, AccumulationTime REAL,
           SummedIntensities INTEGER, MaxIntensity INTEGER);
         CREATE TABLE DiaFrameMsMsInfo (Frame INTEGER PRIMARY KEY, WindowGroup INTEGER);
         CREATE TABLE DiaFrameMsMsWindows (WindowGroup INTEGER, ScanNumBegin INTEGER,
           ScanNumEnd INTEGER, IsolationMz REAL, IsolationWidth REAL, CollisionEnergy REAL);
         CREATE TABLE PasefFrameMsMsInfo (Frame INTEGER, ScanNumBegin INTEGER,
           ScanNumEnd INTEGER, IsolationMz REAL, IsolationWidth REAL,
           CollisionEnergy REAL, Precursor INTEGER);
         CREATE TABLE PrmFrameMsMsInfo (Frame INTEGER, ScanNumBegin INTEGER,
           ScanNumEnd INTEGER, IsolationMz REAL, IsolationWidth REAL,
           CollisionEnergy REAL, Target INTEGER);
         CREATE TABLE PrmTargets (Id INTEGER PRIMARY KEY, ExternalId TEXT, Time REAL,
           OneOverK0 REAL, MonoisotopicMz REAL, Charge INTEGER, Description TEXT);",
    )
    .expect("schema");
    if let Some((lo, hi)) = mz_range {
        conn.execute(
            "UPDATE GlobalMetadata SET Value = ?1 WHERE Key = 'MzAcqRangeLower'",
            [lo],
        )
        .expect("lower");
        conn.execute(
            "UPDATE GlobalMetadata SET Value = ?1 WHERE Key = 'MzAcqRangeUpper'",
            [hi],
        )
        .expect("upper");
    }
    for (i, t) in msms_types.iter().enumerate() {
        conn.execute(
            "INSERT INTO Frames VALUES (?1, ?2, 2, 0, 0, '+', 8, ?3, 1, 100.0, 0, 0)",
            rusqlite::params![i as i64 + 1, (i as f64 + 1.0) * 0.1, t],
        )
        .expect("frame row");
    }
    dir
}

fn open(dir: &PathBuf) -> opentimstdf::mzml::OwnedTdfSource {
    TdfSource::open(dir).expect("open")
}

#[test]
fn prm_pasef_frames_are_counted_and_flagged() {
    // Frames: MS1, PRM (target 7, no PrmTargets row), PRM (no info rows),
    // PRM (target 8, has a row), a type this reader has never seen.
    let dir = bundle("gaps-prm", &[0, 10, 10, 10, 2], None);
    {
        let conn = rusqlite::Connection::open(dir.join("analysis.tdf")).expect("sqlite");
        conn.execute_batch(
            "INSERT INTO PrmFrameMsMsInfo VALUES (2, 1, 10, 600.0, 3.0, 30.0, 7);
             INSERT INTO PrmFrameMsMsInfo VALUES (4, 1, 10, 700.0, 3.0, 30.0, 8);
             INSERT INTO PrmTargets VALUES (8, 'T8', 10.0, 1.0, 700.0, 2, 'pep');",
        )
        .expect("rows");
    }
    let src = open(&dir);
    let gaps = src.projection_gaps();
    assert_eq!(gaps.unprojected_frames.get(&10), Some(&3));
    assert_eq!(gaps.unprojected_frames.get(&2), Some(&1));
    assert_eq!(gaps.frames_without_spectra(), 4);
    assert_eq!(gaps.prm_frames_without_rows, 1);
    assert_eq!(gaps.prm_targets_without_row, 1);

    let rm = src.run_metadata();
    let by_code = |code: &str| {
        rm.warnings
            .iter()
            .find(|w| w.kind.code() == code)
            .unwrap_or_else(|| panic!("no run warning {code}: {:?}", rm.warnings))
    };
    let spectra = by_code(FRAMES_NOT_PROJECTED_CODE);
    assert!(
        spectra.message.contains("4 frames produced no spectrum")
            && spectra.message.contains("3 with msms_type 10")
            && spectra.message.contains("1 with msms_type 2"),
        "{}",
        spectra.message
    );
    let chrom = by_code(PRM_CHROMATOGRAM_GAPS_CODE);
    assert!(
        chrom
            .message
            .contains("1 prm-PASEF frames with no PrmFrameMsMsInfo row")
            && chrom
                .message
                .contains("1 PRM targets with no PrmTargets row"),
        "{}",
        chrom.message
    );
}

#[test]
fn dia_and_pasef_frames_without_rows_are_counted() {
    let dir = bundle("gaps-dia", &[0, 9, 9, 8, 8], None);
    {
        let conn = rusqlite::Connection::open(dir.join("analysis.tdf")).expect("sqlite");
        conn.execute_batch(
            "INSERT INTO DiaFrameMsMsInfo VALUES (2, 1);
             INSERT INTO DiaFrameMsMsWindows VALUES (1, 1, 10, 500.0, 25.0, 30.0);
             INSERT INTO PasefFrameMsMsInfo VALUES (4, 1, 10, 500.0, 2.0, 30.0, 1);",
        )
        .expect("rows");
    }
    let src = open(&dir);
    let gaps = src.projection_gaps();
    assert!(gaps.unprojected_frames.is_empty());
    assert_eq!(
        gaps.dia_frames_without_windows, 1,
        "frame 3 has no window group"
    );
    assert_eq!(
        gaps.pasef_frames_without_rows, 1,
        "frame 5 has no PASEF row"
    );
    let rm = src.run_metadata();
    assert_eq!(rm.warnings.len(), 1, "{:?}", rm.warnings);
    assert_eq!(rm.warnings[0].kind.code(), FRAMES_NOT_PROJECTED_CODE);
    assert!(rm.warnings[0]
        .message
        .contains("1 diaPASEF with no window group"));
}

#[test]
fn a_bundle_with_nothing_left_out_has_no_run_warning() {
    let dir = bundle("gaps-none", &[0, 0], None);
    let src = open(&dir);
    assert_eq!(src.projection_gaps().frames_without_spectra(), 0);
    assert!(src.run_metadata().warnings.is_empty());
}

#[test]
fn acquisition_range_is_the_stated_values_or_an_error() {
    let dir = bundle("gaps-range", &[0], None);
    let reader = opentimstdf::Reader::open(&dir).expect("open");
    assert_eq!(
        reader.mz_acquisition_range().expect("range"),
        Some((100.0, 1700.0))
    );

    // The calibration is read at open, so change the keys afterwards.
    let conn = rusqlite::Connection::open(dir.join("analysis.tdf")).expect("sqlite");
    conn.execute(
        "UPDATE GlobalMetadata SET Value = 'abc' WHERE Key = 'MzAcqRangeLower'",
        [],
    )
    .expect("update");
    let err = reader.mz_acquisition_range().expect_err("unparseable");
    assert!(
        matches!(err, opentimstdf::Error::InvalidMetadata(_)) && err.to_string().contains("NaN"),
        "{err}"
    );
    conn.execute(
        "DELETE FROM GlobalMetadata WHERE Key = 'MzAcqRangeLower'",
        [],
    )
    .expect("delete");
    let err = reader.mz_acquisition_range().expect_err("one key only");
    assert!(err.to_string().contains("needs both"), "{err}");
    conn.execute(
        "DELETE FROM GlobalMetadata WHERE Key = 'MzAcqRangeUpper'",
        [],
    )
    .expect("delete");
    assert_eq!(reader.mz_acquisition_range().expect("absent"), None);
}
