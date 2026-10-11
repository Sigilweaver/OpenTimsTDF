//! Decode failures surface as `Err` items instead of being skipped.
//!
//! Builds tiny synthetic codec-2 bundles (a SQLite `analysis.tdf` plus a
//! hand-made `analysis.tdf_bin`) in the Cargo target tmp dir, so it needs no
//! corpus: one with a corrupt middle frame, one with a zero-byte
//! `analysis.tdf_bin`, and one with a payload-less block for a frame that
//! lists peaks.

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

/// Write a bundle with `bin` as `analysis.tdf_bin` and one MS1 `Frames`
/// row (2 scans) per `(TimsId, NumPeaks)` pair.
fn write_bundle(dir: &Path, bin: &[u8], frames: &[(i64, u32)]) {
    fs::create_dir_all(dir).expect("mkdir");
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
    for (i, (off, num_peaks)) in frames.iter().enumerate() {
        conn.execute(
            "INSERT INTO Frames VALUES (?1, ?2, 2, ?3, ?4, '+', 8, 0, 1, 100.0, 120, 70)",
            rusqlite::params![i as i64 + 1, (i as f64 + 1.0) * 0.1, num_peaks, off],
        )
        .expect("frame row");
    }
}

fn build_bundle(dir: &Path) {
    let mut bin = Vec::new();
    let mut frames = Vec::new();
    // Frame 2 claims 3 scans in its header but 2 in `Frames`: corrupt.
    for header_scans in [2u32, 3, 2] {
        frames.push((bin.len() as i64, 2));
        bin.extend(frame_block(header_scans));
    }
    write_bundle(dir, &bin, &frames);
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

/// Three frames that list peaks, and a zero-byte `analysis.tdf_bin` (as in
/// an incomplete upload).
fn empty_bin_bundle(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.d"));
    write_bundle(&dir, &[], &[(0, 2), (100, 2), (200, 2)]);
    dir
}

#[test]
fn empty_tdf_bin_is_an_err_for_every_frame() {
    let dir = empty_bin_bundle("empty-bin-iter");
    let reader = opentimstdf::Reader::open(&dir).expect("open");
    let frame = reader.frame(1).expect("frame 1");
    let err = reader.decode_peaks(&frame).expect_err("no frame data");
    assert!(
        matches!(err, opentimstdf::Error::CorruptFrame(1, _)),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("past the end of analysis.tdf_bin"),
        "{err}"
    );

    let mut src = TdfSource::open(&dir).expect("open source");
    let items: Vec<_> = src.iter_spectra().collect();
    assert_eq!(items.len(), 3, "one item per frame");
    for (i, item) in items.iter().enumerate() {
        let err = item.as_ref().expect_err("frame has no data");
        assert!(
            err.to_string()
                .contains(&format!("corrupt frame {}", i + 1)),
            "{err}"
        );
    }
}

#[test]
fn write_mzml_fails_on_empty_tdf_bin() {
    let dir = empty_bin_bundle("empty-bin-write");
    let mut out = Vec::new();
    let err = opentimstdf::mzml::write_mzml(&dir, &mut out).expect_err("no frame data");
    assert!(err.to_string().contains("corrupt frame 1"), "{err}");
    let mut out = Vec::new();
    let err = opentimstdf::mzml::write_indexed_mzml(&dir, &mut out).expect_err("no frame data");
    assert!(err.to_string().contains("corrupt frame 1"), "{err}");
}

#[test]
fn empty_block_with_listed_peaks_is_an_err() {
    // Frame 1: an 8-byte block (no payload) with NumPeaks = 0 is a valid
    // empty frame. Frame 2: the same block with NumPeaks = 2 is missing
    // its peaks.
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("empty-block.d");
    let mut bin = Vec::new();
    for _ in 0..2 {
        bin.extend_from_slice(&8u32.to_le_bytes());
        bin.extend_from_slice(&2u32.to_le_bytes());
    }
    write_bundle(&dir, &bin, &[(0, 0), (8, 2)]);
    let reader = opentimstdf::Reader::open(&dir).expect("open");
    let empty = reader.frame(1).expect("frame 1");
    assert!(reader.decode_peaks(&empty).expect("empty frame").is_empty());
    let missing = reader.frame(2).expect("frame 2");
    let err = reader.decode_peaks(&missing).expect_err("peaks missing");
    assert!(
        matches!(err, opentimstdf::Error::CorruptFrame(2, _)),
        "{err:?}"
    );
}
