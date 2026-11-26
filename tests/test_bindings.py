"""Real-file smoke test for the canonical Bruker Python record surface."""

from __future__ import annotations

import os
import sqlite3
from pathlib import Path

import opentimstdf
import pytest


def _corpus_bundle() -> Path:
    path = Path(os.environ.get("OPENTIMSTDF_TEST_BUNDLE", "corpus/NQO1-F107C_coi-N2-P_200-0C_3996.d"))
    if not path.is_dir():
        if os.environ.get("REQUIRE_CORPUS", "").strip() in {"1", "true", "all"}:
            pytest.fail(f"REQUIRE_CORPUS is set but {path} is not a TDF bundle")
        pytest.skip("set OPENTIMSTDF_TEST_BUNDLE to a TDF bundle")
    return path


def test_canonical_records():
    path = _corpus_bundle()

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


def test_ms2_record_keys():
    """The Core 4 record dict on a real PASEF MS2 record."""
    reader = opentimstdf.Reader(str(_corpus_bundle()))
    run = reader.run_info()
    assert run["mobility_array_kind"] == "inverse_reduced_k0"

    records = reader.iter_records()
    ms1 = next(records)
    assert ms1["ms_level"] == 1 and ms1["precursor"] is None
    ms2 = next(r for r in records if r["ms_level"] == 2)

    # Collision energy is a list of {value, unit}; the Core 3 keys are gone.
    assert "collision_energy" not in ms2["precursor"]
    assert "ce_is_nce" not in ms2["precursor"]
    energies = ms2["precursor"]["collision_energies"]
    assert len(energies) == 1
    assert energies[0]["unit"] == "ev"
    assert 0.0 < energies[0]["value"] < 200.0

    # Labels are Core's own, retention time is a float on a decoded frame.
    assert ms2["analyzer"] == ms2["precursor"]["analyzer"] == "tof"
    assert ms2["scan_mode"] == "centroid"
    assert ms2["precursor"]["activation"] == "hcd"
    assert ms2["polarity"] == "positive"
    assert isinstance(ms2["retention_time_sec"], float)
    assert ms2["precursor"]["selected_mz"] > 0
    assert ms2["precursor"]["precursor_native_id"].startswith("frame=")
    assert len(ms2["mz"]) == len(ms2["intensity"])
    assert "warnings" not in ms2

    # The scan window is the m/z acquisition range the bundle states.
    con = sqlite3.connect(f"file:{_corpus_bundle() / 'analysis.tdf'}?mode=ro", uri=True)
    stated = dict(
        con.execute(
            "SELECT Key, Value FROM GlobalMetadata WHERE Key LIKE 'MzAcqRange%'"
        ).fetchall()
    )
    con.close()
    window = {
        "lower": float(stated["MzAcqRangeLower"]),
        "upper": float(stated["MzAcqRangeUpper"]),
    }
    assert ms1["scan_windows"] == [window]
    assert ms2["scan_windows"] == [window]


def test_empty_spectrum_warning():
    """A PASEF range with no decoded peaks is flagged, not silently empty."""
    path = _corpus_bundle()
    if path.name != "NQO1-F107C_coi-N2-P_200-0C_3996.d":
        pytest.skip("empty-spectrum position is known only for the CI bundle")
    reader = opentimstdf.Reader(str(path))
    # Frame 20, scan 502-527 is the first PASEF range with no peaks.
    empty = next(r for r in reader.iter_records() if r["native_id"] == "frame=20 scan=502-527")
    assert len(empty["mz"]) == 0
    assert empty["base_peak_mz"] is None and empty["low_mz"] is None
    assert [w["kind"] for w in empty["warnings"]] == ["empty_spectrum"]


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


def test_unprojected_frames_are_flagged_in_run_info(tmp_path):
    """prm-PASEF frames produce no spectrum; the run says how many."""
    bundle = tmp_path / "prm.d"
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
        INSERT INTO Frames VALUES (1, 0.1, 2, 0, 0, '+', 10, 0, 1, 100.0, 0, 0);
        INSERT INTO Frames VALUES (2, 0.2, 2, 0, 0, '+', 10, 10, 1, 100.0, 0, 0);
        INSERT INTO Frames VALUES (3, 0.3, 2, 0, 0, '+', 10, 10, 1, 100.0, 0, 0);
        """
    )
    con.commit()
    con.close()
    run = opentimstdf.Reader(str(bundle)).run_info()
    codes = {w["kind"]: w["message"] for w in run["warnings"]}
    assert "2 with msms_type 10" in codes["opentimstdf.frames_not_projected"]
