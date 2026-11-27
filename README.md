# OpenTimsTDF

[![CI](https://github.com/Sigilweaver/OpenTimsTDF/actions/workflows/ci.yml/badge.svg)](https://github.com/Sigilweaver/OpenTimsTDF/actions/workflows/ci.yml)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.20470603.svg)](https://doi.org/10.5281/zenodo.20470603)
[![crates.io](https://img.shields.io/crates/v/opentimstdf.svg)](https://crates.io/crates/opentimstdf)
[![PyPI](https://img.shields.io/pypi/v/opentimstdf.svg)](https://pypi.org/project/opentimstdf/)
[![docs.rs](https://img.shields.io/docsrs/opentimstdf)](https://docs.rs/opentimstdf)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/LICENSE)
[![Rust MSRV](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)

> Part of the [OpenMassSpec](https://github.com/Sigilweaver/OpenMassSpec)
> stack for mass spectrometry raw-file access.

Rust and Python reader for timsTOF `.d/` (TDF) acquisition bundles -
the SQLite `analysis.tdf` metadata file and the `analysis.tdf_bin`
binary frame stream. Runs on Linux, macOS, and Windows.

Documentation: [sigilweaver.app/opentimstdf/docs](https://sigilweaver.app/opentimstdf/docs)

## Install

**Prefer [`openmassspec-io`](https://github.com/Sigilweaver/OpenMassSpec)
with the `bruker` feature/extra** unless you need this parser standalone
(minimal dependencies, or building your own abstraction) - the umbrella
gives you format auto-detection, mzML conversion, and Arrow streaming
across all wired-in vendors for free:

```sh
cargo add openmassspec-io --features bruker
```

```sh
pip install openmassspec[bruker]
```

Standalone:

Rust:

```sh
cargo add opentimstdf
```

Python:

```sh
pip install opentimstdf
```

## Quickstart

Rust:

```rust
use opentimstdf::Reader;

let reader = Reader::open("my_bundle.d")?;
let calib = reader.calibration()?;
let frame = reader.frame(1)?;
for peak in reader.decode_peaks(&frame)? {
    let mz = calib.tof_to_mz(peak.tof);
    let im = calib.scan_to_inv_mobility(peak.scan);
    println!("scan={} mz={:.4} 1/K0={:.4} i={}", peak.scan, mz, im, peak.intensity);
}
```

Python:

```python
import opentimstdf

reader = opentimstdf.Reader("my_bundle.d")
calib = reader.calibration()
frame = reader.frame(1)
for peak in reader.decode_peaks(frame):
    mz = calib.tof_to_mz(peak.tof)
    print(peak.scan, mz, peak.intensity)
```

See the [docs site](https://sigilweaver.app/opentimstdf/docs) for the
full quickstart, guide, and format specification.

## Known issues and gaps

- **m/z accuracy is limited to the bundle's stored calibration.** m/z is
  computed from the bundle's `MzCalibration` table. On the public test
  corpus (9 bundles), calibrant and siloxane ions read about 1.9 ppm from
  their true m/z (median), 10.4 ppm at the 95th percentile and 16 ppm at
  most; on some bundles the error grows toward high m/z. Against the
  precursor m/z values the acquisition software stored in each bundle the
  median is about 0.8 ppm, which shows the decoder reproduces the
  instrument's own calibration, not that the calibration is accurate. No
  temperature correction is applied. For accurate-mass work below about
  5 ppm, recalibrate against known masses.
- **Bundles without a usable `MzCalibration` table** (for example
  `ModelType = 2`) use a range-based fallback model. On PXD035173, the one
  such bundle in the corpus, it reads 14 to 31 ppm off; applied to bundles
  that do have tables, its median error ranged from 2 to about 200 ppm.
  `Reader::mz_calibration_status()` and the `opentimstdf.mz_calibration`
  run metadata key report which model was used. See the
  [calibration notes](https://sigilweaver.app/opentimstdf/docs/format/calibration).
- **Missing frame data is an error.** A frame whose block in
  `analysis.tdf_bin` is missing, truncated or does not hold the peaks
  `Frames` lists is an `Err` item when iterating spectra, and `write_mzml`
  stops at the first one. A bundle with an empty `analysis.tdf_bin` (an
  incomplete upload) fails on its first frame.
- **PRM-PASEF frames are not converted to spectra.** `MsMsType = 10`
  frames are skipped by `iter_spectra` and mzML; they feed only the SRM
  chromatograms. The corpus has PRM-PASEF metadata but no frame data. The
  frames are counted: the run carries the warning
  `opentimstdf.frames_not_projected` with the count (and
  `TdfSource::projection_gaps()` returns it), and the warning
  `opentimstdf.prm_chromatogram_gaps` counts prm-PASEF frames with no
  `PrmFrameMsMsInfo` row and targets with no `PrmTargets` row. The same
  warning counts diaPASEF frames with no window group and PASEF frames with
  no `PasefFrameMsMsInfo` row. See #13.
- **Collision energy is one value in eV per spectrum.** `PasefFrameMsMsInfo`
  and `DiaFrameMsMsWindows` carry the collision energy without a unit column.
  The bundle's own `PropertyDefinitions` table gives `eV` as the dimension of
  every collision-energy setting, and the stored values fall inside the
  acquisition's collision-energy ramp, so records state `ev`.
  `CollisionEnergySweepingInfo` (stepped sweeps) is not read; no public bundle
  has rows in it.
- **A PASEF spectrum whose parent MS1 frame is not written is rejected.**
  `Precursors.Parent` names the MS1 frame a precursor was selected from, and
  the record's `spectrumRef` points at that frame's spectrum. When that frame
  is not in the output (it failed to decode while the writer skips bad records,
  or `Parent` names no MS1 frame), Core's mzML writer reports a dangling
  precursor reference. Check name `dangling_precursor_ref`:
  `WriteOptions::with_leniency` with `Check::DanglingPrecursorRef` writes the
  spectrum without the reference and adds a warning.
- **Spectra with no peaks are written empty and flagged.** A mobility range
  or frame with no decoded peaks (664 of 23,293 MS2 spectra in the CI bundle)
  carries Core's warning `empty_spectrum`, and declares no base peak or m/z
  range.
- **Observed m/z can lie outside the scan window.** Every spectrum's
  `scanWindowList` holds the stated `MzAcqRangeLower` to `MzAcqRangeUpper`
  from the bundle's `GlobalMetadata`, as stored; it is not widened and not
  checked against the peaks. The stated range does not bound them: in the
  three public bundles (all recorded by `Bruker otofControl`), observed m/z
  reaches up to 5.0 beyond the limits on both sides. Spectra with a peak
  outside the window: PXD036417 5,086 of 24,425 (window 300 to 1600),
  PXD031833 10,934 of 34,287 (300 to 1600) and PXD025576 63,768 of 83,525
  (99.992696 to 1700). Do not use the scan window as a filter or as the
  limit of the observed m/z range; read the peaks themselves. A bundle
  without the keys has no scan window.
- **Only TDF bundles are read.** TSF bundles (TIMS off, MALDI) and BAF
  bundles are not supported.
- **Tested data:** timsTOF Pro, Pro 2 and SCP, DDA-PASEF and diaPASEF,
  acquired 2019 to 2022, positive mode only. timsTOF HT and Ultra, data
  from 2023 onward (newer acquisition software), negative mode and MALDI
  acquisitions are untested. CI runs against one DDA-PASEF bundle
  (PXD036417); diaPASEF, codec 1 and the range-fallback model are tested
  only against the local corpus.

## License

Apache-2.0. See [LICENSE](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/LICENSE).

The TDF format was worked out from public sample data (PRIDE accessions).
See [ATTRIBUTION.md](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/ATTRIBUTION.md).
Codec 1 decoding is ported from AlphaTims and the codec 2 decode loop
follows OpenTIMS; see
[THIRD-PARTY-NOTICES](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/THIRD-PARTY-NOTICES).
