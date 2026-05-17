# `analysis.tdf` - SQLite schema

The `.tdf` file is an ordinary SQLite3 database (header
`SQLite format 3\0`). The subset of tables needed to walk the binary
stream is documented here; the full schema has many more (for
calibration curves, MALDI frame layouts, DIA windows, PASEF
precursors, ...). Mode-specific tables are described in
[05-instrument-tables.md](05-instrument-tables.md).

## `GlobalMetadata` (key/value)

| Key | Type | Meaning |
| --- | ---- | ------- |
| `SchemaVersionMajor`             | str of int | e.g. `"3"` |
| `SchemaVersionMinor`             | str of int | e.g. `"1"`, `"3"`, `"5"`, `"6"`, `"7"` |
| `InstrumentName`                 | str | e.g. `"timsTOF Pro"`, `"timsTOF SCP"`, `"timsTOF Ultra"` |
| `InstrumentFamily`               | str | e.g. `"timsTOF"` |
| `AcquisitionSoftware`            | str | e.g. `"timsControl"` |
| `AcquisitionSoftwareVersion`     | str | e.g. `"2.0.18"` |
| `TimsCompressionType`            | str of int | **1 = raw, 2 = zstd.** See [03-frame-payload-encoding.md](03-frame-payload-encoding.md). |
| `MzAcqRangeLower` / `Upper`      | str of float | precursor m/z acquisition range |
| `OneOverK0AcqRangeLower` / `Upper` | str of float | 1/K0 acquisition range |
| `DigitizerNumSamples`            | str of int | full TOF range of the digitizer |
| ...                              | ...  | many more - instrument / method / calibration etc. |

## `Frames` - authoritative frame index

Every row in `Frames` points at exactly one block in `analysis.tdf_bin`.

Observed columns (superset across sv 3.1 ... 3.7):

```
Id, Time, Polarity, ScanMode, MsMsType, TimsId,
MaxIntensity, SummedIntensities, NumScans, NumPeaks,
MzCalibration, T1, T2, TimsCalibration, PropertyGroup,
AccumulationTime, RampTime
```

Columns that matter for the binary stream:

| Column | Type | Meaning |
| ------ | ---- | ------- |
| `Id`         | INT   | Frame index, 1-based, dense. |
| `TimsId`     | INT   | **Byte offset of this frame's block header in `analysis.tdf_bin`.** Verified across all 12 non-empty bundles in the corpus. |
| `NumScans`   | INT   | Number of TIMS scans (ion-mobility bins) in the frame. |
| `NumPeaks`   | INT   | Sum of peaks across all scans of this frame. |
| `Polarity`   | TEXT  | `"+"` or `"-"`. |
| `ScanMode`   | INT   | 0 = MS1, 8 = PASEF, 9 = diaPASEF, 10 = PRM. |
| `MsMsType`   | INT   | 0 = MS1, 2 = MRM/PRM (legacy), 8 = PASEF MS2, 9 = diaPASEF, 10 = prm-PASEF. |
| `Time`       | REAL  | Retention time (seconds from start of acquisition). |
| `AccumulationTime` | REAL | Ion accumulation time in milliseconds (sv >= 3.5; absent in older schemas). |
| `RampTime`   | REAL  | TIMS ramp (elution) time in milliseconds. Equal to `AccumulationTime` in all observed bundles. |
| `SummedIntensities` | INT | Sum of all peak intensities in the frame, **normalised to 100 ms accumulation**: `SummedIntensities = sum(raw_intensity) * 100.0 / AccumulationTime_ms`. For bundles where `AccumulationTime ~ 100` the factor is ~ 1.0. Verified on PXD022216 (AT=108.46 ms): ratio decoded_sum / SummedIntensities = 1.08460 +/- 0.00001 across all 57,886 frames. |
| `MzCalibration` | INT | Foreign key into `MzCalibration` table. |
| `TimsCalibration` | INT | Foreign key into `TimsCalibration` table. |

## Mode-specific index tables

| Table | When present | Meaning |
| ----- | ------------ | ------- |
| `DiaFrameMsMsInfo` + `DiaFrameMsMsWindows` | DIA / diaPASEF acquisitions | Per-frame window ID + isolation window table |
| `PasefFrameMsMsInfo` | PASEF (DDA) acquisitions | Per-frame precursor scan range + IM range + CE |
| `PrmFrameMsMsInfo` + `PrmTargets` | All acquisitions (sv >= 3.1); populated only when PRM is active | Per-frame PRM target + isolation; target list |
| `Precursors` | PASEF DDA | Monoisotopic m/z, charge, intensity, parent frame / scan |
| `FrameMsMsInfo`  | All acquisitions (legacy pre-PASEF; always empty in modern corpus) | Frame, Parent, TriggerMass, IsolationWidth, PrecursorCharge, CollisionEnergy |
| `MaldiFrameInfo` | MALDI timsTOF | XY spot coordinates (out of scope) |
| `FrameProperties` + `PropertyDefinitions` | All acquisitions (sv >= 3.1) | Per-frame instrument telemetry (source, vacuum, voltages) |
| `GroupProperties` + `PropertyGroups` | All acquisitions | Per-group set-point values (acquisition config; analogous to FrameProperties but at group level) |
| `Segments` | All acquisitions | Acquisition segment registry (frame range + calibration-segment flag) |
| `ErrorLog` | All acquisitions | Per-frame / scan acquisition error or warning messages |
| `CollisionEnergySweepingInfo` | sv >= 3.7 | Per-frame CE sweep step table |
| `CalibrationInfo` | All acquisitions | Calibration audit trail (datetime, user, reference masses, measured TOFs) |

Detail for each of these is in [05-instrument-tables.md](05-instrument-tables.md).

**Multi-calibration.** `MzCalibration` may have more than one row.
Instruments that support polarity switching store one calibration per
polarity (observed in a sv=3.3 bundle: Id=1 for `"+"`, Id=2 for `"-"`).
The `Frames.MzCalibration` FK always selects the correct row;
implementations must follow the FK rather than assuming Id=1.

**Multi-segment acquisitions.** Every bundle in the corpus (all schema
versions) has exactly one row in
`Segments(Id, FirstFrame, LastFrame, IsCalibrationSegment)` with
`IsCalibrationSegment = 0`. All `Frames` rows reference
`TimsCalibration` Id=1. Multi-segment runs (recalibration mid-acquisition)
are supported by the schema but not yet observed in the corpus. See
[05-instrument-tables.md](05-instrument-tables.md#segments).

## Calibration tables

### `MzCalibration`

One row per calibration ID. All corpus bundles have at least one row.

```
Id                INTEGER  primary key
ModelType         INTEGER  always 1 (TOF polynomial model)
DigitizerTimebase REAL     digitizer sample interval in ns (observed: 0.2)
DigitizerDelay    REAL     digitizer delay in samples (observed: ~25000-44000)
T1                REAL     TOF correction coefficient 1
T2                REAL     TOF correction coefficient 2
dC1               REAL     distortion correction 1 (often 20.0-21.0)
dC2               REAL     distortion correction 2 (often 0.0)
C0                REAL     polynomial coefficient 0 (offset)
C1                REAL     polynomial coefficient 1 (linear term)
C2                REAL     polynomial coefficient 2 (quadratic term)
C3                REAL     polynomial coefficient 3 (often 0.0)
C4                REAL     polynomial coefficient 4 (small correction)
```

Sample row (PXD027359, sv=3.5):
```
(1, 1, 0.2, 25581.8, 25.304233, 22.467882, 21.0, 0.0,
 308.058693, 156618.35376, -0.003637, 0.0, 0.045331)
```

The proprietary evaluation formula for `mz = f(tof, ...)` is not
implemented by OpenTimsTDF. See [04-calibration.md](04-calibration.md)
for the open-source fallback.

### `TimsCalibration`

One row per calibration ID. `ModelType = 2`.

```
Id         INTEGER  primary key
ModelType  INTEGER  always 2 (TIMS polynomial model)
C0         REAL     1 (polarity flag / scan_offset_A)
C1         REAL     approx. MAX(NumScans)-1 (scan reference)
C2 .. C9   REAL     polynomial coefficients
```

Sample row (PXD027359, sv=3.5):
```
(1, 2, 1.0, 926.0, 220.548, 75.423, 33.333, 1.0,
 0.02452, 132.436, 13.074, 2768.110)
```

The proprietary evaluation formula for `1/K0 = f(scan, ...)` is not
published. See [04-calibration.md](04-calibration.md) for the
open-source fallback.
