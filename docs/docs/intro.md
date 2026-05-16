---
sidebar_position: 1
slug: /
---

# OpenTDF

**Pure-Rust parser for Bruker timsTOF `.d/` (TDF) bundles, reverse-engineered without the vendor SDK.**

OpenTDF reads the binary frame stream and SQLite metadata produced by
Bruker timsTOF instruments and exposes them through a small, ergonomic
Rust API. It has no dependency on the closed-source Bruker SDK and runs
on Linux, macOS, and Windows.

## What it covers

| Component                                            | Status |
| ---------------------------------------------------- | ------ |
| `analysis.tdf` SQLite metadata                       | full   |
| `analysis.tdf_bin` block stream                      | full   |
| Codec 2 (zstd + byte-transpose + delta) frame decode | full   |
| Codec 1 (LZF + signed-delta) frame decode            | full   |
| TOF to m/z + scan to 1/K0 calibration                | full (open-source linear-in-sqrt(m/z) model, < 2 ppm vs. vendor) |
| diaPASEF window metadata                             | full   |
| PASEF DDA precursors + MS/MS info                    | full   |
| prm-PASEF targets + per-frame info                   | full   |
| Schema versions 3.1, 3.3, 3.5, 3.6, 3.7              | full   |
| `analysis.tsf` (MALDI)                               | out of scope |

## Next steps

- [Install](./install) the crate.
- Run through the [Quickstart](./quickstart).
- Read the [Format specification](./format/overview) if you want to
  understand or extend the binary layer.
- Browse the API on [docs.rs](https://docs.rs/opentdf).

## License

OpenTDF is Apache-2.0 licensed. See [License](./license).
