---
sidebar_position: 1
---

# Reader

`opentimstdf::Reader` is the entry point. Opening a bundle parses the
`GlobalMetadata` table, validates the schema version, and prepares
SQLite statements for repeated frame access.

## Opening a bundle

```rust
use opentimstdf::Reader;
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
| `calibration()`                       | `Result<Calibration>`             | TOF/m/z and scan/(1/K0) calibrators for the most-referenced `MzCalibration` row (see [Calibration](./calibration)). |
| `bundle_calibration()`                | `Result<BundleCalibration>`       | Calibration per `MzCalibration` row; `for_frame(&frame)` picks a frame's row. |
| `mz_calibration_model()`              | `Result<MzCalibrationModel>`      | `Tables` or `RangeFallback`. |
| `mz_calibration_status()`             | `Result<MzCalibrationStatus>`     | Model plus the reason when the range fallback is used. |
| `range_fallback_calibration()`        | `Result<Calibration>`             | The range-fallback model, built even when the tables are usable (for comparison). |
| `frame(id)`                           | `Result<Frame>`                   | A single frame's index row. |
| `frames()`                            | `Result<Vec<Frame>>`              | All frames in ascending id order. |
| `decode_peaks(&frame)`                | `Result<Vec<Peak>>`               | Decode a frame's payload to `(scan, tof, intensity)` peaks. See [Peaks and codecs](./peaks-and-codecs). |
| `dia_windows_for_frame(id)`           | `Result<Option<DiaFrameWindows>>` | diaPASEF isolation windows; `None` for non-DIA bundles. |
| `pasef_msms_info_for_frame(id)`       | `Result<Vec<PasefMsMsInfo>>`      | DDA PASEF per-frame info. |
| `prm_msms_info_for_frame(id)`         | `Result<Vec<PrmMsMsInfo>>`        | prm-PASEF per-frame entries. |
| `prm_target(target_id)`               | `Result<Option<PrmTarget>>`       | prm-PASEF target metadata. |
| `precursor(precursor_id)`             | `Result<Option<Precursor>>`       | A row from the `Precursors` table. |

See the full type signatures on [docs.rs](https://docs.rs/opentimstdf).

## Types and errors

The structs these methods return (`Frame`, `Peak`, `Metadata`,
`DiaWindow`, `PasefMsMsInfo`, `Precursor`, ...) are `#[non_exhaustive]`.
Read their fields directly; new fields can be added in a minor release, so
code outside the crate cannot build them with struct literals (`Peak` has
`Peak::new`).

`opentimstdf::Error` is also `#[non_exhaustive]`, so a `match` on it needs a
`_` arm. A frame whose payload cannot be decoded returns
`Error::CorruptFrame`, and a missing or out-of-range `GlobalMetadata` value
needed by the calibration returns `Error::InvalidMetadata`. SQLite failures
return `Error::Sqlite`, whose `Display` gives the SQLite message.

## mzML export

```rust
let mut out = std::io::BufWriter::new(std::fs::File::create("run.mzML")?);
let summary = opentimstdf::mzml::write_indexed_mzml("my_bundle.d", &mut out)?;
println!("{} spectra", summary.spectra_written);
```

`write_mzml` and `write_indexed_mzml` stop at the first frame or
chromatogram that cannot be decoded and return its error. To write
everything else and count what was left out, build a `TdfSource` and use
the `openmassspec-core` writer with `RecordErrorPolicy::Skip`:

```rust
use openmassspec_core::{RecordErrorPolicy, WriteOptions};

let mut src = opentimstdf::mzml::TdfSource::open("my_bundle.d")?;
let opts = WriteOptions::new().with_record_error_policy(RecordErrorPolicy::Skip);
let summary = openmassspec_core::write_mzml_with_options(&mut src, &mut out, &opts)?;
if summary.skipped() > 0 {
    eprintln!("{} records could not be decoded", summary.skipped());
}
```

`TdfSource::iter_spectra` yields one `Err` item per frame that fails and
then continues with the next frame.
