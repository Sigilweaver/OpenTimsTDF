---
sidebar_position: 1
---

# Reader

`opentdf::Reader` is the entry point. Opening a bundle parses the
`GlobalMetadata` table, validates the schema version, and prepares
SQLite statements for repeated frame access.

## Opening a bundle

```rust
use opentdf::Reader;
let reader = Reader::open("my_bundle.d")?;
```

`Reader::open` takes any path that resolves to a `.d/` directory and
contains both `analysis.tdf` (SQLite) and `analysis.tdf_bin` (block
stream).

## Methods

| Method | Returns | Notes |
| ------ | ------- | ----- |
| `open(path)`                          | `Result<Reader>`                  | Open and validate a bundle. |
| `bundle_dir()`                        | `&Path`                           | The path the reader was opened on. |
| `metadata()`                          | `Result<Metadata>`                | All `GlobalMetadata` rows decoded into a struct. |
| `compression_type()`                  | `u32`                             | `1` (LZF) or `2` (zstd). |
| `calibration()`                       | `Result<Calibration>`             | TOF/m/z and scan/(1/K0) calibrators (see [Calibration](./calibration)). |
| `frame(id)`                           | `Result<Frame>`                   | A single frame's index row. |
| `frames()`                            | `Result<Vec<Frame>>`              | All frames in ascending id order. |
| `decode_peaks(&frame)`                | `Result<Vec<Peak>>`               | Decode a frame's payload to `(scan, tof, intensity)` peaks. See [Peaks and codecs](./peaks-and-codecs). |
| `dia_windows_for_frame(id)`           | `Result<Option<DiaFrameWindows>>` | diaPASEF isolation windows; `None` for non-DIA bundles. |
| `pasef_msms_info_for_frame(id)`       | `Result<Vec<PasefMsMsInfo>>`      | DDA PASEF per-frame info. |
| `prm_msms_info_for_frame(id)`         | `Result<Vec<PrmMsMsInfo>>`        | prm-PASEF per-frame entries. |
| `prm_target(target_id)`               | `Result<Option<PrmTarget>>`       | prm-PASEF target metadata. |
| `precursor(precursor_id)`             | `Result<Option<Precursor>>`       | A row from the `Precursors` table. |

See the full type signatures on [docs.rs](https://docs.rs/opentdf).
