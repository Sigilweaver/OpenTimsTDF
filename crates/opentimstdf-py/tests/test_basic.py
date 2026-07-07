"""Smoke tests for the opentimstdf Python bindings.

These exercise the import surface and run against a `.d/` bundle if one is
available via the `OpenTimsTDF_TEST_BUNDLE` env var. Without a bundle, only the
import/version checks run.
"""

from __future__ import annotations

import os

import numpy as np

import opentimstdf


def test_version() -> None:
    assert isinstance(opentimstdf.__version__, str)
    assert opentimstdf.__version__.count(".") >= 1


def test_classes_present() -> None:
    for name in [
        "Reader",
        "Calibration",
        "Frame",
        "Peak",
        "Metadata",
        "DiaWindow",
        "DiaFrameWindows",
        "PasefMsMsInfo",
        "PrmMsMsInfo",
        "PrmTarget",
        "Precursor",
    ]:
        assert hasattr(opentimstdf, name), f"missing class: {name}"


def test_bundle_roundtrip() -> None:
    bundle = os.environ.get("OpenTimsTDF_TEST_BUNDLE")
    if not bundle:
        return

    reader = opentimstdf.Reader(bundle)
    meta = reader.metadata()
    assert meta.schema_version_major >= 0

    calib = reader.calibration()
    # Round trip a TOF value.
    tof = 100_000
    mz = calib.tof_to_mz(tof)
    assert mz > 0
    tof_back = calib.mz_to_tof(mz)
    assert abs(int(tof_back) - tof) <= 2

    frames = reader.frames()
    assert len(frames) > 0
    first = frames[0]
    peaks = reader.decode_peaks(first)
    assert len(peaks) == first.num_peaks


def test_decode_spectrum_returns_numpy_arrays() -> None:
    bundle = os.environ.get("OpenTimsTDF_TEST_BUNDLE")
    if not bundle:
        return

    reader = opentimstdf.Reader(bundle)
    frames = reader.frames()
    assert len(frames) > 0
    first = next(f for f in frames if f.num_peaks > 0)

    spec = reader.decode_spectrum(first)
    assert len(spec) == first.num_peaks
    for arr, dtype in (
        (spec.mz, np.float64),
        (spec.inv_mobility, np.float64),
        (spec.intensity, np.uint32),
    ):
        assert isinstance(arr, np.ndarray)
        assert arr.dtype == dtype
        assert arr.shape == (first.num_peaks,)
        # Zero-copy: the array should reference the Rust-owned buffer
        # rather than a copy allocated by NumPy itself.
        assert arr.flags["OWNDATA"] is False
