---
sidebar_position: 1
slug: /
---

# OpenTDF

OpenTDF is a Rust library that reads timsTOF `.d/` (TDF) acquisition
bundles - the SQLite `analysis.tdf` metadata file and the
`analysis.tdf_bin` binary frame stream. The format and codecs were
worked out from public sample data (PRIDE accessions); no proprietary
SDK or vendor source was consulted.

OpenTDF runs on Linux, macOS, and Windows, with optional Python
bindings via [`opentdf-py`](./install).

## What it covers

| Component                                            | Status |
| ---------------------------------------------------- | ------ |
| `analysis.tdf` SQLite metadata                       | supported |
| `analysis.tdf_bin` block stream                      | supported |
| Codec 2 (zstd + byte-transpose + delta) frame decode | supported |
| Codec 1 (LZF + signed-delta) frame decode            | supported |
| TOF to m/z + scan to 1/K0 calibration                | supported (linear-in-sqrt(m/z) model) |
| diaPASEF window metadata                             | supported |
| PASEF DDA precursors + MS/MS info                    | supported |
| prm-PASEF targets + per-frame info                   | supported |
| Schema versions 3.1, 3.3, 3.5, 3.6, 3.7              | supported |
| `analysis.tsf` (MALDI)                               | out of scope |

## Next steps

- [Install](./install) the Rust crate or the Python package.
- Run through the [Quickstart](./quickstart).
- Read the [Format specification](./format/overview) for the binary
  layer.
- Browse the API on [docs.rs](https://docs.rs/opentdf).

## License

OpenTDF is Apache-2.0 licensed. See [License](./license).
