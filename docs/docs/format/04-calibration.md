# Calibration

`MzCalibration` and `TimsCalibration` hold proprietary polynomial
models that are not publicly documented. OpenTDF instead implements
open-source linear-in-sqrt(m/z) and linear-in-scan models that are
sufficient for typical analyses (< 2 ppm m/z error when calibrated
from `CalibrationInfo`).

## TOF -> m/z (boundary variant)

Follows the OpenSource model implemented by `opentims`
(`tof2mz_converter.cpp`, BSD-2-Clause):

```
mz_min  = GlobalMetadata.MzAcqRangeLower
mz_max  = GlobalMetadata.MzAcqRangeUpper
tof_max = GlobalMetadata.DigitizerNumSamples

if GlobalMetadata.AcquisitionSoftware == "Bruker otofControl":
    mz_min -= 5
    mz_max += 5

intercept = sqrt(mz_min)
slope     = (sqrt(mz_max) - sqrt(mz_min)) / tof_max

mz(tof) = (intercept + slope * tof)^2
```

The inverse is `tof(mz) = (sqrt(mz) - intercept) / slope`.

**Not** the proprietary polynomial stored in `MzCalibration`.
The 13-column coefficient table holds everything needed for a
closed-form evaluation, but the formula is not publicly documented
and no open-source implementation is known. Consumers that need to
reproduce the proprietary calibration must use the vendor toolchain.

**Verified** (`calibration_ranges_match_metadata`): for the
PXD027359 bundle, `tof_to_mz(0) = 100.0` to within 1e-6 and
`tof_to_mz(DigitizerNumSamples) = 1700.0` to within 1e-3, matching
`GlobalMetadata.MzAcqRange{Lower,Upper}` exactly.

## TOF -> m/z (regressed variant)

When `CalibrationInfo` is present (it is in all observed bundles),
the slope and intercept can be fit directly from ground-truth pairs:

```
tof_idx_i  = (MeasuredTimesOfFlight_i - DigitizerDelay) / DigitizerTimebase
sqrt(mz_i)                                                              -- from ReferencePeakMasses
[fit slope, intercept by least squares on (tof_idx_i, sqrt(mz_i))]

mz(tof) = (intercept + slope * tof)^2
```

`DigitizerDelay` and `DigitizerTimebase` are columns of `MzCalibration`
(not `GlobalMetadata`). This regressed form is equivalent to
`Tof2MzConverter::regress_from_pairs()` in `timsrust` and achieves
< 2 ppm max error across all six tested bundles (3 to 5 pairs each).

The boundary variant can be off by up to ~15000 ppm when
`DigitizerDelay` is large relative to the digitizer range; it should
be used only as a fallback when `CalibrationInfo` is absent.

### MzCalibration field meanings (open-source perspective)

| Field | Meaning |
| ----- | ------- |
| `T1` | linear slope in units of 100 ns / sqrt(Da). `t_ns / 100 ~= T1 * sqrt(mz) + T2`. Matches fitted slope to within 0.4%. |
| `T2` | nominal (pre-calibration) intercept in the same unit system. |
| `C0` | fitted flight-time zero offset in nanoseconds when C2 = C4 = 0. |
| `C1 - C4` | polynomial correction coefficients for the proprietary `ModelType=1` evaluation; not needed for the open-source linear formula. |

## Scan -> 1/K0 (linear)

Follows the OpenSource model implemented by `opentims`
(`scan2inv_ion_mobility_converter.cpp`, BSD-2-Clause):

```
im_min    = GlobalMetadata.OneOverK0AcqRangeLower
im_max    = GlobalMetadata.OneOverK0AcqRangeUpper
scan_max  = MAX(NumScans) FROM Frames     -- largest NumScans observed

intercept = im_max
slope     = (im_min - im_max) / scan_max

one_over_k0(scan) = intercept + slope * scan
```

Inverse: `scan(1/K0) = (1/K0 - intercept) / slope`.

`TimsCalibration(C0 .. C9)` holds a proprietary polynomial.
`ModelType = 2` uses a 10-coefficient rational model; `C0 = 1` acts
as a polarity / offset flag and `C1` is close to `MAX(NumScans) - 1`.
The evaluation formula is not publicly documented. OpenTDF uses the
linear model documented above.

**Verified** (`calibration_ranges_match_metadata`):
`scan_to_inv_mobility(0) = 1.6` and
`scan_to_inv_mobility(MAX(NumScans)) = 0.6` to within 1e-9.

## Higher-order corrections

No higher-order correction has been identified from `CalibrationInfo`
for the scan -> 1/K0 mapping. `MeasuredTimsVoltages` (polarity = `"-"`)
stores TIMS exit voltages for the reference 1/K0 peaks, but the
voltage-to-scan-index mapping requires the
`TimsCalibration.ModelType = 2` polynomial (C0 .. C9) and has not been
decoded. All known open-source tools use the linear model.
