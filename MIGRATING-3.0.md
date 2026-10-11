# Migrating to opentimstdf 3.0

3.0 is the last planned breaking release: after it, new fields, enum
variants, calibration models and error variants arrive in minor releases.
The full list of changes is in [CHANGELOG.md](CHANGELOG.md). Python users
only need the last section.

## Dependency

```toml
[dependencies]
opentimstdf = "3"
openmassspec-core = "3"   # if you use TdfSource or the core writer directly
```

## Calibration

`Calibration` no longer has public fields and is no longer `Copy`.

| 2.0 | 3.0 |
| --- | --- |
| `cal.im_intercept`, `cal.im_slope` | `cal.mobility_conversion()` (match `MobilityConversion::AcquisitionRange { intercept, slope, .. }`) |
| `cal.mz_intercept`, `cal.mz_slope` | `cal.mz_conversion()` (match `MzConversion::RangeFallback { intercept, slope, .. }`) |
| `Calibration { .. }` literal | `Calibration::new(mz, mobility)` with `MzConversion::range_fallback(i, s)` or `MzConversion::Tables(TablesMzModel::new(..))` and `MobilityConversion::acquisition_range(i, s)` |
| `let c = *reader.calibration()?` / `*bundle.primary()` | `reader.calibration()?` / `bundle.primary().clone()` |
| `*bundle.for_frame(&frame)` | `bundle.for_frame(&frame)` (returns an owned `Calibration`) |
| `bundle.status` | `bundle.status()` |

The conversion methods (`tof_to_mz`, `mz_to_tof`, `scan_to_inv_mobility`,
`inv_mobility_to_scan` and the `_f64` variants) are unchanged. Matches on
`MzConversion`, `MobilityConversion` and `MzCalibrationModel` need a `_`
arm.

## Records from `TdfSource`

`iter_spectra` and `iter_chromatograms` yield `Result` items. A frame that
cannot be decoded is an `Err` item instead of being skipped:

```rust
use openmassspec_core::SpectrumSource;

let mut src = opentimstdf::mzml::TdfSource::open("run.d")?;
for rec in src.iter_spectra() {
    let rec = rec?;
    // ...
}
```

`mzml::write_mzml` / `write_indexed_mzml` return
`Result<openmassspec_core::WriteSummary>` and stop at the first error.
`let _ = write_mzml(..)?;` or `write_mzml(..)?;` keeps working. To skip
undecodable frames instead, use `openmassspec_core::write_mzml_with_options`
with `RecordErrorPolicy::Skip` and check `summary.skipped()`.

## Data types and errors

`Frame`, `Peak`, `Metadata`, `DiaWindow`, `DiaFrameWindows`,
`PasefMsMsInfo`, `PrmMsMsInfo`, `PrmTarget` and `Precursor` are
`#[non_exhaustive]`. Reading fields is unchanged; struct literals outside
the crate no longer compile. Use `Peak::new(scan, tof, intensity)`; the
other types come from `Reader` methods.

`Error` is `#[non_exhaustive]`: add a `_` arm to matches. `Error::Sqlite`
now holds an opaque `SqliteError` (its `Display` is the SQLite message), and
there is no `From<rusqlite::Error>` impl. Calibration metadata problems are
`Error::InvalidMetadata` (they were `CorruptFrame(0, ..)` or `Sqlite`).
`Error::Core` carries errors from `openmassspec-core`, and
`opentimstdf::Error` converts into `openmassspec_core::Error` with `?`.

`decode_codec2` returns `Result<Vec<Peak>, String>`, like `decode_codec1`.

## Python

No renamed or removed names. Behavior changes:

- `Reader.iter_records()` raises `RuntimeError` on the first frame that
  cannot be decoded, and `Reader.read_chromatograms()` raises when the
  chromatograms cannot be built. Both used to leave the data out.
- `Calibration.im_intercept` / `im_slope` raise `RuntimeError` if a bundle's
  1/K0 model is not linear (none is today).
