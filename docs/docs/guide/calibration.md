---
sidebar_position: 2
---

# Calibration

OpenTimsTDF uses the linear-in-sqrt(m/z) calibration model from the
[opentims](https://github.com/michalsta/opentims) and
[rustims](https://github.com/theGreatHerrLebert/rustims) projects.

## API

```rust
let calib = reader.calibration()?;
let mz = calib.tof_to_mz(tof_index);
let one_over_k0 = calib.scan_to_inv_mobility(scan_index);
```

## What is read

`Reader::calibration()` reads the acquisition ranges from
`GlobalMetadata` (`MzAcqRangeLower/Upper`, `DigitizerNumSamples`,
`OneOverK0AcqRangeLower/Upper`) and the largest `Frames.NumScans`, and
builds one linear-in-sqrt(m/z) and one linear 1/K0 mapping for the whole
run. Per-frame `MzCalibration` coefficients and `CalibrationInfo`
reference peaks are not used.

## Accuracy

This range-based m/z model is approximate. The format notes record
errors of up to ~15000 ppm on some bundles; the size depends on the
bundle. The calibration fitted from `CalibrationInfo` (< 2 ppm on the
tested bundles) is not implemented. For accurate-mass work, recalibrate
against known masses or use wide m/z tolerances.

For details of the calibration tables and the exact mathematical model,
see the format spec:

- [04-calibration.md](../format/calibration)
- [01-tdf-sqlite-schema.md](../format/tdf-sqlite-schema) (`MzCalibration`, `MobilityCalibration`)
