# OpenTimsTDF

[![CI](https://github.com/Sigilweaver/OpenTimsTDF/actions/workflows/ci.yml/badge.svg)](https://github.com/Sigilweaver/OpenTimsTDF/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/opentimstdf.svg)](https://crates.io/crates/opentimstdf)
[![PyPI](https://img.shields.io/pypi/v/opentimstdf.svg)](https://pypi.org/project/opentimstdf/)
[![docs.rs](https://img.shields.io/docsrs/opentimstdf)](https://docs.rs/opentimstdf)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust MSRV](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

> Part of the [OpenProteo](https://sigilweaver.app/openproteo/docs/)
> stack for proteomics raw-file access. Sibling readers:
> [OpenWRaw](https://github.com/Sigilweaver/OpenWRaw) (Waters),
> [OpenTFRaw](https://github.com/Sigilweaver/OpenTFRaw) (Thermo).

Rust and Python reader for timsTOF `.d/` (TDF) acquisition bundles -
the SQLite `analysis.tdf` metadata file and the `analysis.tdf_bin`
binary frame stream. Runs on Linux, macOS, and Windows.

Documentation: [sigilweaver.app/opentimstdf/docs](https://sigilweaver.app/opentimstdf/docs)

## Install

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

## License

Apache-2.0. See [LICENSE](LICENSE).

The TDF format and codecs were worked out from public sample data
(PRIDE accessions). See [ATTRIBUTION.md](ATTRIBUTION.md).
