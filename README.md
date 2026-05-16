# OpenTDF

[![CI](https://github.com/Sigilweaver/OpenTDF/actions/workflows/ci.yml/badge.svg)](https://github.com/Sigilweaver/OpenTDF/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/opentdf.svg)](https://crates.io/crates/opentdf)
[![PyPI](https://img.shields.io/pypi/v/opentdf.svg)](https://pypi.org/project/opentdf/)
[![docs.rs](https://img.shields.io/docsrs/opentdf)](https://docs.rs/opentdf)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)

Rust and Python reader for timsTOF `.d/` (TDF) acquisition bundles -
the SQLite `analysis.tdf` metadata file and the `analysis.tdf_bin`
binary frame stream. Runs on Linux, macOS, and Windows.

**Full documentation: [sigilweaver.app/opentdf/docs](https://sigilweaver.app/opentdf/docs)**

## Install

```toml
# Cargo.toml
[dependencies]
opentdf = "0.1"
```

```sh
pip install opentdf
```

## Quick start

```rust
use opentdf::Reader;

let reader = Reader::open("my_bundle.d")?;
let calib = reader.calibration()?;
let frame = reader.frame(1)?;
for peak in reader.decode_peaks(&frame)? {
    let mz = calib.tof_to_mz(peak.tof);
    let im = calib.scan_to_inv_mobility(peak.scan);
    println!("scan={} mz={:.4} 1/K0={:.4} i={}", peak.scan, mz, im, peak.intensity);
}
```

```python
import opentdf

reader = opentdf.Reader("my_bundle.d")
calib = reader.calibration()
frame = reader.frame(1)
for peak in reader.decode_peaks(frame):
    mz = calib.tof_to_mz(peak.tof)
    print(peak.scan, mz, peak.intensity)
```

See [Quickstart](https://sigilweaver.app/opentdf/docs/quickstart),
[Guide](https://sigilweaver.app/opentdf/docs/guide/reader), and
[Format specification](https://sigilweaver.app/opentdf/docs/format/overview).

## License

Apache-2.0. See [LICENSE](LICENSE). Copyright 2026 Sigilweaver Holdings LLC.

The TDF format and codecs were worked out from public sample data
(PRIDE accessions). See [ATTRIBUTION.md](ATTRIBUTION.md).
