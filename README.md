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
  chromatograms. The corpus has PRM-PASEF metadata but no frame data.
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
