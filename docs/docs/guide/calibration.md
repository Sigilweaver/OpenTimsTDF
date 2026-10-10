---
sidebar_position: 2
---

# Calibration

OpenTimsTDF converts TOF indices to m/z with the calibration stored in the
bundle's `MzCalibration` table, and scan indices to 1/K0 with a linear
model from the acquisition mobility range.

## API

```rust
let calib = reader.calibration()?;
let mz = calib.tof_to_mz(tof_index);
let one_over_k0 = calib.scan_to_inv_mobility(scan_index);

// Which m/z model is in use, and why if it is the fallback.
let status = reader.mz_calibration_status()?;
println!("{} {:?}", status.model, status.fallback_reason);
```

`calibration()` returns the calibration of the `MzCalibration` row that
most frames reference. Frames of different polarity reference different
rows; to convert any frame correctly, use the per-frame lookup:

```rust
let cal = reader.bundle_calibration()?;
for frame in reader.frames()? {
    let c = cal.for_frame(&frame);
    for p in reader.decode_peaks(&frame)? {
        let mz = c.tof_to_mz(p.tof);
    }
}
```

The mzML writer and the Python `decode_spectrum` already do this.

## What is read

- **m/z (tables model, the default)**: `MzCalibration.DigitizerDelay`,
  `DigitizerTimebase`, `C0`, `C1`, `C2`, `C4` of each row referenced by
  `Frames.MzCalibration`.
- **m/z (range fallback)**: `GlobalMetadata.MzAcqRangeLower/Upper` and
  `DigitizerNumSamples`. Used only when the `MzCalibration` table is
  absent or unusable (for example `ModelType` other than 1). The reason is
  returned by `mz_calibration_status()`, written to run metadata as
  `opentimstdf.mz_calibration_fallback_reason`, and logged as a warning.
- **1/K0**: `GlobalMetadata.OneOverK0AcqRangeLower/Upper` and the largest
  `Frames.NumScans`.

Run metadata `extra` always carries `opentimstdf.mz_calibration` with
`tables` or `range_fallback`.

## Accuracy

On the public bundles tested, the tables model's median error against the
precursor m/z values stored in the bundle is 0.8 to 0.9 ppm (up to 3.2 ppm
on one test fixture). The range fallback ranges from 2 ppm to about 200
ppm depending on the bundle. No temperature correction is applied, and an
instrument that drifted after calibration reads high or low by that drift
(one tested bundle reads 9 to 18 ppm high). For accurate-mass work below
about 5 ppm, recalibrate against known masses.

For the model, the validation method, and per-bundle figures, see:

- [04-calibration.md](../format/calibration)
- [01-tdf-sqlite-schema.md](../format/tdf-sqlite-schema) (`MzCalibration`, `TimsCalibration`)
