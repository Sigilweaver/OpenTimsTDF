"""Real-file smoke test for the canonical Bruker Python record surface."""

from __future__ import annotations

import os
import sqlite3
from pathlib import Path

import opentimstdf
import pytest


def test_canonical_records():
    path = Path(os.environ.get("OPENTIMSTDF_TEST_BUNDLE", "corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d"))
    if not path.is_dir():
        if os.environ.get("REQUIRE_CORPUS", "").strip() in {"1", "true", "all"}:
            pytest.fail(f"REQUIRE_CORPUS is set but {path} is not a TDF bundle")
        pytest.skip("set OPENTIMSTDF_TEST_BUNDLE to a TDF bundle")

    reader = opentimstdf.Reader(str(path))
    run = reader.run_info()
    assert run["source_file_name"] == path.name
    assert run["source_file_format"]["accession"]
    assert "opentimstdf.schema_version_major" in run["extra"]

    model = reader.mz_calibration_model()
    assert model in {"tables", "range_fallback"}
    assert run["extra"]["opentimstdf.mz_calibration"] == model
    reason = reader.mz_calibration_fallback_reason()
    assert (reason is None) == (model == "tables")
    calib = reader.calibration()
    assert calib.mz_model == model
    if model == "tables":
        assert calib.mz_intercept is None and calib.mz_slope is None
    tof = 100_000
    assert abs(calib.mz_to_tof(calib.tof_to_mz(tof)) - tof) <= 1

    record = next(reader.iter_records())
    assert record["native_id"]
    assert len(record["mz"]) == len(record["intensity"])
    assert "opentimstdf.frame_id" in record["extra"]
    assert record["scan_number"] >= 1
    frame = reader.frames()[0]
    assert frame.polarity_symbol in {"+", "-"}
    assert frame.polarity in {"positive", "negative"}
    assert frame.id >= 1 and frame.num_scans > 0
    assert isinstance(calib.im_intercept, float) and calib.im_slope < 0
    spectrum = reader.decode_spectrum(frame)
    assert len(spectrum) == len(reader.decode_peaks(frame)) == frame.num_peaks

    for chrom in reader.read_chromatograms():
        assert len(chrom["time_sec"]) == len(chrom["intensity"])


def _empty_bin_bundle(root: Path) -> Path:
    """Synthetic bundle: two MS1 frames that list peaks and a zero-byte
    analysis.tdf_bin, as in an incomplete upload."""
    bundle = root / "empty-bin.d"
    bundle.mkdir()
    (bundle / "analysis.tdf_bin").write_bytes(b"")
    con = sqlite3.connect(bundle / "analysis.tdf")
    con.executescript(
        """
        CREATE TABLE GlobalMetadata (Key TEXT PRIMARY KEY, Value TEXT);
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
        INSERT INTO Frames VALUES (1, 0.1, 2, 2, 0, '+', 8, 0, 1, 100.0, 120, 70);
        INSERT INTO Frames VALUES (2, 0.2, 2, 2, 100, '+', 8, 0, 1, 100.0, 120, 70);
        """
    )
    con.commit()
    con.close()
    return bundle


def test_empty_tdf_bin_raises(tmp_path):
    reader = opentimstdf.Reader(str(_empty_bin_bundle(tmp_path)))
    frame = reader.frames()[0]
    with pytest.raises(RuntimeError, match="past the end of analysis.tdf_bin"):
        reader.decode_peaks(frame)
    with pytest.raises(RuntimeError, match="corrupt frame 1"):
        next(reader.iter_records())
