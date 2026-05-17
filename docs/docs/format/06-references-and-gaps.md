# References and gaps

## References

- **opentims** - https://github.com/michalsta/opentims (BSD-2-Clause).
  Validates the raw scan-offset header layout (codec 2 inner layout)
  and supplies the open-source linear calibration model.
- **alphatims** - https://github.com/MannLabs/alphatims (MIT).
  Documents the codec 1 frame header and delta + bit-pack scheme;
  source of the codec 1 verbatim implementation.
- **rustims** - https://github.com/theGreatHerrLebert/rustims (MIT).
  `rustdf/src/data/handle.rs` `LinearFrameConverter` uses the same
  linear model documented in [04-calibration.md](04-calibration.md).
- **Public format references** - the publicly distributed PaSER schema
  reference is consistent with what we observe on the
  `DiaFrameMsMs*` / `PasefFrameMsMsInfo` tables.

## Out-of-scope items

### MALDI (`analysis.tsf`)

MALDI timsTOF acquisitions produce `analysis.tsf` bundles. The `.tsf`
format uses a different SQLite schema: spectra are indexed by spot
position rather than retention time, there is no scan (1/K0)
dimension, and the binary data layout is different. Supporting
`.tsf` would require a parallel implementation targeting a distinct
format and acquisition modality (MALDI vs ESI / nanoESI). The
`opentimstdf` crate targets `.tdf` (ESI / nanoESI timsTOF) only. A
separate crate (e.g. `opentsf`) would be the appropriate scope.

### Proprietary polynomial models

**`MzCalibration` polynomial (ModelType=1).** A proprietary non-linear
TOF -> m/z mapping uses coefficients `C1 - C4` stored in
`CalibrationInfo`. These coefficients are visible in the open
database but the functional form is not publicly documented. The
open-source linear approximation achieves < 2 ppm for typical
acquisitions (see
[04-calibration.md](04-calibration.md#tof---mz-regressed-variant)).

**`TimsCalibration` voltage polynomial (ModelType=2).** A proprietary
model uses 10 coefficients stored in `CalibrationInfo` blobs
(`MeasuredTimsVoltages`, `MeasuredTimsCurrents`) to map voltage to
scan index. The functional form is not publicly documented. The
open-source linear approximation is sufficient for all analyses not
requiring sub-scan-index 1/K0 accuracy.

Implementing the proprietary models is an explicit non-goal. The path
forward is to find published `(tof_index, mz)` or `(scan_index, 1/K0)`
pairs from open literature or open datasets and fit the 13 or 10
coefficient model directly.

### Multi-segment calibration (schema only, hypothetical)

The `Segments` table supports N rows with associated `TimsCalibration`
entries (one per segment). All 64 probe bundles have exactly one
segment. Multi-segment calibration is a schema feature that may be
used in very long acquisitions or split-gradient experiments but no
example has been observed in the public corpus. The parser handles
it correctly (by accepting any `TimsCalibration` row), but the feature
has not been exercised.

### Per-polarity calibration (partial)

`MzCalibration` may have one row per polarity (`Id=1` for `+`,
`Id=2` for `-`). `Frames.MzCalibration` identifies which polarity
row applies. The open-source linear approximation reads only
`GlobalMetadata` acquisition-range keys
(`MzAcqRangeLower`, `MzAcqRangeUpper`), which are single values for
the whole acquisition. Per-polarity differentiation would require
the proprietary polynomial model, which is unavailable in the open
schema. A multi-polarity sv=3.3 bundle was observed in the probe
corpus; the parser reads it correctly, but only the calibration
accuracy for the non-primary polarity is affected by this
limitation.

### Stepped collision-energy sweeps

`CollisionEnergySweepingInfo` is present in all sv >= 3.7 bundles by
schema but never populated in any of the 81 probe / public bundles
checked. No populated example has been found in any public dataset;
the feature may be used only in instrument-internal validation runs
or future modes not yet publicly released.
