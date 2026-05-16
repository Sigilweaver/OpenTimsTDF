# OpenTDF

[![CI](https://github.com/Sigilweaver/OpenTDF/actions/workflows/ci.yml/badge.svg)](https://github.com/Sigilweaver/OpenTDF/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/opentdf.svg)](https://crates.io/crates/opentdf)
[![docs.rs](https://img.shields.io/docsrs/opentdf)](https://docs.rs/opentdf)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)

Pure-Rust parser for Bruker timsTOF `.d/` (TDF) mass-spectrometry
bundles, reverse-engineered without the vendor SDK. Runs on Linux,
macOS, and Windows.

**Full documentation: [sigilweaver.app/opentdf/docs](https://sigilweaver.app/opentdf/docs)**

## Install

```toml
[dependencies]
opentdf = "0.1"
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

More: [Quickstart](https://sigilweaver.app/opentdf/docs/quickstart)
| [Guide](https://sigilweaver.app/opentdf/docs/guide/reader)
| [Format spec](https://sigilweaver.app/opentdf/docs/format/overview)
| [API on docs.rs](https://docs.rs/opentdf).

## License

Apache-2.0. See [LICENSE](LICENSE). Copyright 2026 Sigilweaver Holdings LLC.

Builds on prior open-source reverse-engineering work; see
[ATTRIBUTION.md](ATTRIBUTION.md).
