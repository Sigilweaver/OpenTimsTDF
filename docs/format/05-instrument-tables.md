# Instrument-specific tables

Acquisition-mode-specific tables documented and verified against the
probe corpus.

## DIA / diaPASEF

Presence: `DiaFrameMsMsWindows` + `DiaFrameMsMsInfo` +
`DiaFrameMsMsWindowGroups`. Verified against PXD025576
(sv=3.1, DIAMAX method, codec=2, ScanMode=9).

PASEF DDA bundles (ScanMode=8) include these tables but leave them
empty. diaPASEF bundles (ScanMode=9) have `PasefFrameMsMsInfo` and
`Precursors` absent entirely; isolation metadata lives entirely in
the DIA tables.

**`DiaFrameMsMsInfo`** - one row per MS2 frame:
```
Frame        INTEGER  frame ID (FK into Frames; only ScanMode=9, MsMsType=9 frames)
WindowGroup  INTEGER  FK into DiaFrameMsMsWindowGroups.Id
```

**`DiaFrameMsMsWindowGroups`** - registry of window group IDs:
```
Id  INTEGER  primary key
```

**`DiaFrameMsMsWindows`** - isolation windows within a group:
```
WindowGroup     INTEGER  FK into DiaFrameMsMsWindowGroups.Id
ScanNumBegin    INTEGER  first scan of this isolation window (inclusive)
ScanNumEnd      INTEGER  last scan of this isolation window (inclusive)
IsolationMz     REAL     centre m/z of the precursor isolation window
IsolationWidth  REAL     full width of the isolation window (Da)
CollisionEnergy REAL     collision energy (eV) applied during this window
```

In a diaPASEF acquisition each window group has multiple rows that
tile the full scan (1/K0) range. Within a group the scan ranges are
non-overlapping and contiguous. The isolation m/z steps across groups
so that the product of `groups x windows-per-group` covers the full
targeted m/z range.

Sample data (PXD025576, DIAMAX, 16 groups x 4 windows = 64 rows total):
```
-- window group 1
(1,   0, 268, 1012.5, 25.0, 53.68)
(1, 269, 440,  812.5, 25.0, 42.08)
(1, 441, 611,  612.5, 25.0, 33.02)
(1, 612, 774,  412.5, 25.0, 24.22)
-- window group 2
(2,   0, 256, 1037.5, 25.0, 54.00)
(2, 257, 425,  837.5, 25.0, 42.77)
...
```

The 1/K0-m/z co-isolation pattern (higher m/z at higher 1/K0)
exploits the natural correlation between peptide charge state, m/z,
and ion mobility, minimising chimeric spectra. `CollisionEnergy`
scales with `IsolationMz` to optimise fragmentation across the m/z
range.

`DiaFrameMsMsInfo` links each MS2 frame to one window group. The 16
groups cycle continuously (frames 2-17 use groups 1-16, frames 18-33
repeat, ...). MS1 frames (MsMsType=0) have no entry in
`DiaFrameMsMsInfo`.

## PASEF DDA

Presence: `PasefFrameMsMsInfo` + `Precursors`.

**`PasefFrameMsMsInfo`** - one row per MS2 scan range within a
PASEF frame:

```
Frame           INTEGER  frame ID (FK into Frames)
ScanNumBegin    INTEGER  first scan isolating this precursor (inclusive)
ScanNumEnd      INTEGER  last scan isolating this precursor (inclusive)
IsolationMz     REAL     centre m/z of isolation window
IsolationWidth  REAL     full width of isolation window (Da)
CollisionEnergy REAL     collision energy (eV)
Precursor       INTEGER  FK into Precursors.Id
```

Sample rows (PXD027359, sv=3.5, 87138 total rows):
```
(2, 566, 591, 621.082, 2.0, 35.172, 1)
(2, 720, 745, 353.903, 2.0, 28.438, 2)
(2, 775, 800, 281.178, 2.0, 26.034, 3)
```

**`Precursors`** - one row per detected precursor ion:

```
Id              INTEGER  primary key
LargestPeakMz   REAL     m/z of the most intense isotopologue
AverageMz       REAL     intensity-weighted average m/z
MonoisotopicMz  REAL     monoisotopic m/z (NULL if not determined)
Charge          INTEGER  charge state (NULL if not determined)
ScanNumber      REAL     centroid scan (fractional; maps to 1/K0)
Intensity       REAL     summed precursor intensity
Parent          INTEGER  parent MS1 frame ID
```

`MonoisotopicMz` and `Charge` are NULL when the deconvolution
algorithm cannot determine them (common for low-intensity or
co-eluting precursors).

## prm-PASEF

Verified against PXD028279 (Brzhozovskiy et al., Anal. Chem. 2022;
prm-PASEF on timsTOF Pro, sv=3.5, timsControl 2.0.18.0). The `.d`
bundle contains 26,171 MS1 frames and 10,570 PRM frames
(MsMsType=10, ScanMode=10) against 250 scheduled targets.

PRM frames have `MsMsType=10` and `ScanMode=10` in `Frames`. The
`DiaFrameMsMsInfo` table is empty (diaPASEF and PRM are mutually
exclusive within an acquisition).

**`PrmFrameMsMsInfo`** (34,418 rows in PXD028279):

```
Frame           INTEGER  FK into Frames.Id; each PRM frame has one row per active target
ScanNumBegin    INTEGER  first scan index of the isolation window
ScanNumEnd      INTEGER  last scan index of the isolation window
IsolationMz     REAL     precursor m/z centre of the isolation window
IsolationWidth  REAL     isolation window width in Da (3.0 in PXD028279)
CollisionEnergy REAL     collision energy in eV
Target          INTEGER  FK into PrmTargets.Id
```

Multiple rows per frame are expected when more than one target is
measured per frame.

**`PrmTargets`** (250 rows in PXD028279):

```
Id              INTEGER  primary key
ExternalId      TEXT     user-supplied target label (e.g. peptide name or compound ID)
Time            REAL     scheduled retention time (min)
OneOverK0       REAL     expected inverse ion mobility (1/K0)
MonoisotopicMz  REAL     monoisotopic precursor m/z
Charge          INTEGER  precursor charge state
Description     TEXT     free-text annotation
```

**`PrmFrameMeasurementMode`** (10,570 rows in PXD028279):

```
Frame             INTEGER  FK into Frames.Id
MeasurementModeId TEXT     measurement-mode identifier; NULL in all observed rows
```

All 10,570 rows have `MeasurementModeId = NULL` in PXD028279. The
column presumably encodes online vs offline measurement-mode
switching, but no non-NULL value has been observed.

`PrmFrameMsMsInfo` and `PrmTargets` are present in 54 of 64 probe
corpus bundles (spanning sv 3.1 to 3.7) but are always empty when
PRM is not active.

## Legacy DDA (`FrameMsMsInfo`)

Presence: all acquisitions (all 64 probe corpus bundles). Always
empty. Superseded by `PasefFrameMsMsInfo` for PASEF DDA and by the
`Dia*` tables for diaPASEF.

```
Frame           INTEGER  frame ID
Parent          INTEGER  parent MS1 frame ID
TriggerMass     REAL     precursor trigger mass
IsolationWidth  REAL     isolation window width
PrecursorCharge INTEGER  charge state (0 = unassigned)
CollisionEnergy REAL     collision energy
```

## `CalibrationInfo`

Presence: all acquisitions.

```
KeyPolarity  TEXT  polarity qualifier; always "+" in corpus
KeyName      TEXT  parameter name (see below)
Value        BLOB  parameter value (TEXT or binary IEEE-754 double array)
```

Observed `KeyName` values (PXD027359):

| KeyName | Value type | Example |
| ------- | ---------- | ------- |
| `CalibrationDateTime` | TEXT | `2020-12-04T11:41:21+01:00` |
| `CalibrationUser` | TEXT | `Demo User` |
| `CalibrationSoftware` | TEXT | `timsTOF` |
| `CalibrationSoftwareVersion` | TEXT | `2.0.40` |
| `MzCalibrationMode` | TEXT of int | `4` |
| `MzStandardDeviationPPM` | TEXT of float | `0.325020` |
| `ReferenceMassList` | TEXT | `TuneMixIsotopes_ADB` |
| `ReferencePeakMasses` | binary f64[] | packed little-endian doubles |
| `MeasuredTimesOfFlight` | binary f64[] | measured TOF values for reference peaks |
| `MeasuredMassPeakIntensities` | binary f64[] | intensities of reference peaks |
| `MassesCorrectedCalibration` | binary f64[] | post-calibration m/z values |
| `MobilityCalibrationDateTime` | TEXT | `2020-12-08T08:28:39+00:00` |
| `MobilityStandardDeviationPercent` | TEXT of float | `1410.782798` |
| `ReferenceMobilityList` | TEXT | `Tuning Mix ES-TOF (ESI)` |
| `ReferencePeakMobilities` | binary f64[] | reference 1/K0 values |
| `MeasuredTimsVoltages` | binary f64[] | measured TIMS voltages for reference peaks |

Binary blobs are arrays of little-endian `f64` (IEEE-754 double)
values; `len_bytes / 8` is the number of reference peaks.

See [04-calibration.md](04-calibration.md#tof---mz-regressed-variant)
for how `MeasuredTimesOfFlight` and `ReferencePeakMasses` together
yield a < 2 ppm m/z calibration.

## `FrameProperties` / `PropertyDefinitions`

Presence: all acquisitions (sv >= 3.1).

**`FrameProperties`**:
```
Frame     INTEGER  frame ID
Property  INTEGER  FK into PropertyDefinitions.Id
Value     REAL     measured value in units given by PropertyDefinitions
```

**`PropertyDefinitions`**:
```
Id                INTEGER  property ID
PermanentName     TEXT     machine-readable name
Type              INTEGER  0 = enum/integer, 1 = real-valued
DisplayGroupName  TEXT     UI group (e.g. "Source", "Vacuum", "Digitizer")
DisplayName       TEXT     human-readable name
DisplayValueText  TEXT     enum legend for Type=0
DisplayFormat     TEXT     printf-style format string
DisplayDimension  TEXT     unit string
Description       TEXT     additional description
```

Representative properties observed across the corpus:

| PermanentName | DisplayName | Unit |
| ------------- | ----------- | ---- |
| `Source_NebulizerCurrentValue` | Nebulizer | Bar |
| `Source_NanoBoosterCurrentValue` | nanoBooster | Bar |
| `Source_DryGasCurrentValue` | Dry Gas | l/min |
| `Source_DryHeaterCurrentValue` | Dry Heater | degC |
| `Source_CapillaryCurrentValue` | Capillary | nA |
| `Vacuum_CurrentFore` | Tunnel Out Vacuum | mBar |
| `Vacuum_CurrentHigh` | TOF Vacuum | mBar |
| `Digitizer_CurrentTemp` | Digitizer Temperature | degC |
| `Digitizer_Summation` | Summation | x |
| `SpectraAcquisition_SegmentId` | Segment Id | - |

Property IDs are not stable across schema versions (sv 3.1 / 3.3
use IDs in the 1000+ range; sv 3.5 / 3.7 use IDs starting at 0).
Always resolve via a `PropertyDefinitions.Id` join.
`FrameProperties` is informational instrument telemetry and is not
required for peak decoding.

**`GroupProperties`** stores per-acquisition-group set-point values
(instrument configuration), analogous to `FrameProperties` but
keyed by property group rather than by frame.

```
PropertyGroup  INTEGER             FK into PropertyGroups.Id
Property       INTEGER             FK into PropertyDefinitions.Id
Value          (no type affinity)  value (stored as INTEGER or REAL)
```

**`PropertyGroups`** registers the available group IDs:
```
Id  INTEGER  primary key
```

All 64 probe corpus bundles have exactly one `PropertyGroup`
(`Id=1`). Across the corpus 1077 unique property IDs appear in
`GroupProperties`, in categories spanning IMS, MSMS, Transfer,
Calibration, TOF, Digitizer, SyringePump, Source, Collision, Energy,
Quadrupole, and others.

## `CollisionEnergySweepingInfo`

Presence: sv >= 3.7 only.

```
Frame                  INTEGER  frame ID
CollisionId            INTEGER  CE step index within the sweep (0-based)
CollisionEnergy        REAL     absolute collision energy (eV)
CollisionEnergyPercent REAL     CE as a fraction of the m/z-dependent optimal CE
```

Present in all sv=3.7 bundles by schema but **never populated** in
any publicly accessible PRIDE dataset. Checked across 81 distinct
`.d` bundles spanning sv 3.1-3.7: every bundle has 0 rows. The
column names suggest it is intended for stepped-CE acquisition
modes where multiple discrete CE values are applied per frame.

## `Segments`

Presence: all acquisitions.

```
Id                   INTEGER  primary key, 1-based
FirstFrame           INTEGER  first frame ID in this segment
LastFrame            INTEGER  last frame ID in this segment
IsCalibrationSegment BOOLEAN  1 if this is a dedicated calibration segment
```

In all 64 probe corpus bundles there is exactly one row, covering
the full acquisition, with `IsCalibrationSegment = 0`. Multi-segment
and calibration-segment acquisitions are supported by the schema but
were not observed in the corpus.

## `ErrorLog`

Presence: all acquisitions. Usually empty.

```
Frame    INTEGER  frame ID where the condition occurred
Scan     INTEGER  scan index within the frame (NULL for frame-level events)
Message  TEXT     compact message string
```

Populated in only one bundle observed (PXD022216, sv=3.1, codec=1,
diaPASEF, 2 rows). The `"I:"` prefix appears to denote "Info" severity;
no `"W:"` or `"E:"` prefixes were observed.

## MALDI (`MaldiFrameInfo`)

MALDI timsTOF acquisitions produce `analysis.tsf` bundles, not
`analysis.tdf`. The `.tsf` format uses a different SQLite schema and
binary data layout and is out of scope for this crate. See
[06-references-and-gaps.md](06-references-and-gaps.md#maldi-analysistsf).
