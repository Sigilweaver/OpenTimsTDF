# Bruker TDF Format - Overview

The Bruker timsTOF acquisition writes a directory with the extension
`.d/` containing a small, well-defined set of files. The
authoritative index is an embedded SQLite database; the bulk peak
data lives in a flat companion stream.

| File | Purpose | Covered here |
| ---- | ------- | ------------ |
| `analysis.tdf`       | SQLite database: schema, metadata, per-frame index | yes (see [01-tdf-sqlite-schema.md](01-tdf-sqlite-schema.md)) |
| `analysis.tdf_bin`   | Flat stream of compressed/raw frame payloads | yes (see [02-tdf-bin-block-stream.md](02-tdf-bin-block-stream.md)) |
| `*.m/` subdirectory  | XML acquisition method files | out of scope |

All multi-byte integers are **little-endian**. Real numbers are IEEE-754
doubles. There is no top-level magic in `analysis.tdf_bin`; the whole
block index lives in the SQLite database.

## Document map

| File | Topic |
| ---- | ----- |
| [00-overview.md](00-overview.md) | This file. |
| [01-tdf-sqlite-schema.md](01-tdf-sqlite-schema.md) | `analysis.tdf` SQLite schema: `GlobalMetadata`, `Frames`, mode-specific index tables, calibration tables. |
| [02-tdf-bin-block-stream.md](02-tdf-bin-block-stream.md) | `analysis.tdf_bin` block layout (8-byte block header, payload, padding). |
| [03-frame-payload-encoding.md](03-frame-payload-encoding.md) | Frame payload codecs: Codec 1 (LZF + signed-delta) and Codec 2 (zstd + byte-transpose). |
| [04-calibration.md](04-calibration.md) | TOF -> m/z and scan -> 1/K0 calibration models, regressed and boundary variants. |
| [05-instrument-tables.md](05-instrument-tables.md) | Acquisition-mode tables: diaPASEF, PASEF DDA, prm-PASEF, properties, segments, error log. |
| [06-references-and-gaps.md](06-references-and-gaps.md) | Prior art, known gaps, and items out of scope. |

## Status

This specification was developed by binary analysis of a 64-bundle
probe corpus spanning Bruker timsTOF schema versions 3.1 through 3.7
and supplemented with the OpenSource reference implementations
listed in [06-references-and-gaps.md](06-references-and-gaps.md).

## Version compatibility

Every claim in this specification was verified against the schema
versions present in the probe corpus:

- **3.1** (Compass 2019 / early 2020 timsTOF Pro)
- **3.3** (2020 / 2021 timsTOF Pro firmware)
- **3.5** (2021 / 2022 timsTOF Pro 2, SCP)
- **3.6** (2022 / 2023 timsTOF HT)
- **3.7** (PXD039066, otofControl 3.1.13)

No schema-version-keyed difference in the outer block header or the
codec dispatch was observed.

**sv=3.7 additions** (PXD039066):

- `CollisionEnergySweepingInfo(Frame, CollisionId, CollisionEnergy,
  CollisionEnergyPercent)` table added (empty in corpus bundle; see
  [05-instrument-tables.md](05-instrument-tables.md#collisionenergysweepinginfo)).
- `GlobalMetadata` extras: `DenoisingEnabled=1`, `DigitizerType=NI5155`,
  `DigitizerSerialNumber`, `PythonIqResult` (sha256), `PythonPluginChecksum`
  (sha256), `PythonPluginName=paser`. These indicate the PaSER real-time
  data-processing pipeline was active.
- `PrmFrameMeasurementMode(Frame INTEGER, MeasurementModeId TEXT)` table
  added. Verified in PXD028279 (prm-PASEF, sv=3.5): 10,570 rows with
  `MeasurementModeId = NULL` throughout.
