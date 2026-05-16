---
sidebar_position: 2
---

# Calibration

OpenTDF ships an open-source calibration model. Vendor software uses a
proprietary calibration; OpenTDF instead uses the linear-in-sqrt(m/z)
model used by [opentims](https://github.com/michalsta/opentims) and
[rustims](https://github.com/theGreatHerrLebert/rustims), which agrees
with the vendor calibration to under 2 ppm on the validation corpus.

## API

```rust
let calib = reader.calibration()?;
let mz = calib.tof_to_mz(tof_index);
let one_over_k0 = calib.scan_to_inv_mobility(scan_index);
```

## What is read

`Calibration::from(reader)` pulls the per-frame calibration coefficients
from `Frames.MzCalibration` plus the global mobility calibration from
`MobilityCalibration` and combines them into a closed-form pair of
functions.

For details of the calibration tables and the exact mathematical model,
see the format spec:

- [04-calibration.md](../format/calibration)
- [01-tdf-sqlite-schema.md](../format/tdf-sqlite-schema) (`MzCalibration`, `MobilityCalibration`)
