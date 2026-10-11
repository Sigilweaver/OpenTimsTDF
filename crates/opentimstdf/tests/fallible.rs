//! Decode failures surface as `Err` items instead of being skipped.
//!
//! Builds a tiny synthetic codec-2 bundle (three MS1 frames, the middle one
//! corrupt) in the Cargo target tmp dir, so it needs no corpus.

use std::fs;
use std::path::{Path, PathBuf};

use openmassspec_core::{RecordErrorPolicy, SpectrumSource, WriteOptions};
use opentimstdf::mzml::TdfSource;

/// Byte-plane transposition of a logical u32 stream (inverse of the
/// decoder's de-transpose).
fn transpose(logical: &[u32]) -> Vec<u8> {
    let n = logical.len();
    let mut out = vec![0u8; 4 * n];
    for (i, v) in logical.iter().enumerate() {
        for b in 0..4 {
            out[b * n + i] = (v >> (8 * b)) as u8;
        }
    }
    out
}

/// One codec-2 frame block: 8-byte header plus zstd payload. Two scans,
/// one peak in scan 0 and one in scan 1.
fn frame_block(num_scans: u32) -> Vec<u8> {
    // header: [unused, 2 * peaks in scan 0]; peaks: (delta, intensity) x 2.
    let logical = [0u32, 2, 1_001, 50, 2_000, 70];
    let payload = zstd::bulk::compress(&transpose(&logical), 3).expect("zstd");
    let block_size = 8 + payload.len() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(&block_size.to_le_bytes());
    out.extend_from_slice(&num_scans.to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

fn build_bundle(dir: &Path) {
    fs::create_dir_all(dir).expect("mkdir");
    let mut bin = Vec::new();
    let mut offsets = Vec::new();
    // Frame 2 claims 3 scans in its header but 2 in `Frames`: corrupt.
    for header_scans in [2u32, 3, 2] {
        offsets.push(bin.len() as i64);
        bin.extend(frame_block(header_scans));
    }
    fs::write(dir.join("analysis.tdf_bin"), bin).expect("write tdf_bin");

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
           SummedIntensities INTEGER, MaxIntensity INTEGER);",
    )
    .expect("schema");
    for (i, off) in offsets.iter().enumerate() {
        conn.execute(
            "INSERT INTO Frames VALUES (?1, ?2, 2, 2, ?3, '+', 8, 0, 1, 100.0, 120, 70)",
            rusqlite::params![i as i64 + 1, (i as f64 + 1.0) * 0.1, off],
        )
        .expect("frame row");
    }
}

fn bundle(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.d"));
    build_bundle(&dir);
    dir
}

#[test]
fn corrupt_frame_is_an_err_item_and_iteration_continues() {
    let dir = bundle("fallible-iter");
    let mut src = TdfSource::open(&dir).expect("open");
    let items: Vec<_> = src.iter_spectra().collect();
    assert_eq!(items.len(), 3, "one item per frame");
    assert!(items[0].is_ok());
    let err = items[1].as_ref().expect_err("frame 2 is corrupt");
    assert!(err.to_string().contains("corrupt frame 2"), "{err}");
    let last = items[2].as_ref().expect("frame 3 decodes");
    assert_eq!(last.native_id, "frame=3 scan=1");
    assert_eq!(last.mz.len(), 2);
}

#[test]
fn write_mzml_fails_on_corrupt_frame_unless_skipping() {
    let dir = bundle("fallible-write");
    let mut out = Vec::new();
    let err = opentimstdf::mzml::write_mzml(&dir, &mut out).expect_err("corrupt frame");
    assert!(err.to_string().contains("corrupt frame 2"), "{err}");

    let mut src = TdfSource::open(&dir).expect("open");
    let opts = WriteOptions::new().with_record_error_policy(RecordErrorPolicy::Skip);
    let mut out = Vec::new();
    let summary =
        openmassspec_core::write_mzml_with_options(&mut src, &mut out, &opts).expect("write");
    assert_eq!(summary.spectra_written, 2);
    assert_eq!(summary.spectra_skipped, 1);
}
