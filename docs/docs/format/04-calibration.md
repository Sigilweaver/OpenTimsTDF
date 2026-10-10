# Calibration

OpenTimsTDF converts TOF indices to m/z with a model read from the
bundle's `MzCalibration` table (the "tables" model). When that table is
absent or unusable it uses a model built from the acquisition m/z range
(the "range fallback") and records why. Scan indices are converted to
1/K0 with a linear model built from the acquisition mobility range.

Which m/z model a bundle uses is reported by
`Reader::mz_calibration_status()` (and `mz_calibration_model()`), and in
mzML/run metadata under `extra`:

| Key | Value |
| --- | ----- |
| `opentimstdf.mz_calibration` | `tables` or `range_fallback` |
| `opentimstdf.mz_calibration_fallback_reason` | Present only for `range_fallback`: why the tables could not be used |

A fallback is also logged as a warning through the Rust `log` crate, and
the Python binding raises a `RuntimeWarning`.

## TOF -> m/z (tables model)

Each frame references one `MzCalibration` row through
`Frames.MzCalibration`. For `ModelType = 1` rows:

```
t(tof) = DigitizerDelay + DigitizerTimebase * tof                  -- ns
t(mz)  = C0 + (1e6 / sqrt(C1)) * sqrt(mz + C4) + C2 * mz           -- ns
```

With `C2 = C4 = 0` this reduces to `mz = C1 * ((t - C0) * 1e-6)^2`.
Writing `s = sqrt(mz + C4)` and `u = t - C0 + C2 * C4`, the second
equation is the quadratic `C2 * s^2 + K * s - u = 0` with
`K = 1e6 / sqrt(C1)`, so

```
s(tof)  = 2 * u / (K + sqrt(K^2 + 4 * C2 * u))
mz(tof) = s^2 - C4
```

The inverse `tof(mz)` evaluates `t(mz)` directly and solves the first
equation for `tof`.

### Where the model comes from

`CalibrationInfo` stores, per polarity, the reference masses used to
calibrate the instrument (`ReferencePeakMasses`), the flight times
measured for them in ns (`MeasuredTimesOfFlight`), and the masses the
resulting calibration assigns to those flight times
(`MassesCorrectedCalibration`). Array values are blobs of little-endian
`f64`. For every `ModelType = 1` row checked, the formula above maps
`MeasuredTimesOfFlight` to `MassesCorrectedCalibration` of the matching
polarity to within 0.01 ppm, which is the rounding of the stored
coefficients. Bundles checked: PXD036417 (timsTOF Pro, both polarities,
one row with `C2`, `C4` non-zero), PXD027359 (timsTOF Pro), PXD080079 and
PXD079489 (timsTOF HT), and the public test fixtures of the `tdfpy` and
`mzdata` projects (`C2` and `C4` non-zero).

### Which rows are used

`Reader::bundle_calibration()` builds one model per `MzCalibration` row
that `Frames` references; `BundleCalibration::for_frame` selects the
frame's row, and the mzML writer and Python `decode_spectrum` use it.
`Reader::calibration()` returns the row referenced by the most frames.
In a dual-polarity run the two polarities reference different rows, and
the rows are not interchangeable: applying the other polarity's row
moves m/z by 500 to 12000 ppm on the bundles above.

The bundle uses the range fallback instead, with the reason recorded,
when any of these hold for a referenced row:

- the `MzCalibration` table is absent, or a referenced row is missing;
- `ModelType` is not 1 (`ModelType = 2`, seen on older `impacTEM`
  bundles, has extra columns and is not implemented);
- `C3` is non-zero (no observed `ModelType = 1` row has one, so its
  meaning is unknown);
- a value is not numeric or not finite, `DigitizerTimebase` or `C1` is
  not positive, or the formula has no positive, increasing solution over
  TOF indices 0 to `GlobalMetadata.DigitizerNumSamples`.

### Not applied: temperature terms

`MzCalibration.T1`, `T2` hold the two device temperatures at calibration
time (`Frames.T1`, `T2` are the same temperatures per frame, matching the
`TOF_DeviceTempCurrentValue1/2` properties), and `dC1`, `dC2` look like
temperature coefficients. No correction using them is applied: no form
was found that is consistent across the tested bundles (see Accuracy).

## TOF -> m/z (range fallback)

Follows the open-source model implemented by `opentims`
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

The inverse is `tof(mz) = (sqrt(mz) - intercept) / slope`. This model
ignores `DigitizerDelay` and the calibration, so its error depends on how
well the acquisition range happens to match the instrument's calibration:
2 to 13 ppm on some bundles, about 200 ppm on others (see Accuracy).
`Reader::range_fallback_calibration()` builds it even when the tables
are usable, for comparison.

## Accuracy

Measured without vendor software, against ground truth inside each
bundle:

- **Precursors**: for the most intense DDA precursors, the TOF centroid
  of the peak at `Precursors.LargestPeakMz` in the parent MS1 frame
  (scans within 12 of `ScanNumber`, intensity-weighted over 7 TOF
  indices), converted to m/z and compared with `LargestPeakMz`.
- **Reference ions**: exact-mass ions present in the data, such as the
  tuning-mix calibrants (622.0290, 922.0098, 1221.9906) and polysiloxanes
  (445.1200, 519.1388).

Median and 95th percentile of |ppm| against `LargestPeakMz` (up to 4000
precursors per bundle):

| Bundle | Instrument | Tables median / p95 | Range fallback median / p95 |
| ------ | ---------- | ------------------- | --------------------------- |
| PXD036417 NQO1 (CI bundle) | timsTOF Pro | 0.90 / 4.1 | 2.1 / 5.0 |
| PXD027359 HeLa 5.6 min DDA | timsTOF Pro | 0.83 / 12.1 | 12.6 / 18.6 |
| PXD080079 2.5 ug DDA | timsTOF HT | 0.86 / 5.5 | 1.8 / 6.3 |
| `tdfpy` `example_dda.d` fixture | timsTOF Pro | 3.15 / 7.9 | 200 / 356 |

The tables error is flat across m/z, so the TOF index needs no offset. A
one-index offset would add 4 to 9 ppm.

Reference ions under the tables model: calibrants within 0.1 to 3.4 ppm
and siloxanes within 4 ppm on PXD027359 (DDA and dia-PASEF runs) and
`example_dda.d`; siloxanes within 2 ppm on PXD080079. On the PXD079489
dia-PASEF run (timsTOF HT), siloxanes read 9 to 18 ppm high under both
models. That bundle was acquired about 29 hours after its stored
calibration and carries no calibration segment, so the bundle contains no
data to correct it.

The remaining per-bundle precursor offset (signed median -3.1 to +0.8
ppm) is not explained. On the three bundles with `dC2 = 0` it matches
`dC1 * (Frames.T1 - MzCalibration.T1)` ppm to within 0.1 ppm, but not on
`example_dda.d` (`dC2 != 0`), and the calibrant ions do not support
applying that correction. For accurate-mass work below about 5 ppm,
recalibrate against known masses.

## Scan -> 1/K0 (linear)

Follows the open-source model implemented by `opentims`
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

`TimsCalibration(C0 .. C9)` holds a polynomial whose evaluation is not
publicly documented. `ModelType = 2` uses a 10-coefficient model;
`C0 = 1` acts as a polarity / offset flag and `C1` is close to
`MAX(NumScans) - 1`. OpenTimsTDF uses the linear model above.

**Verified** (`range_fallback_calibration_matches_metadata`):
`scan_to_inv_mobility(0) = 1.6` and
`scan_to_inv_mobility(MAX(NumScans)) = 0.6` to within 1e-9 on the
PXD027359 bundle.

No higher-order correction has been identified from `CalibrationInfo`
for the scan -> 1/K0 mapping. `MeasuredTimsVoltages` stores TIMS exit
voltages for the reference 1/K0 peaks, but the voltage-to-scan mapping
requires the `TimsCalibration` model, which has not been decoded.
