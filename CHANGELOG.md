# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-05-16

Initial public release.

### Added

- Pure-Rust reader for Bruker timsTOF `.d/` (TDF) bundles, with no
  dependency on the vendor SDK.
- SQLite metadata access via bundled `rusqlite`: `GlobalMetadata`,
  `Frames`, mode-specific index tables, and calibration tables.
- Binary frame decoder supporting both compression codecs:
  - Codec 2 (`TimsCompressionType = 2`): zstd + byte-transpose + delta.
  - Codec 1 (`TimsCompressionType = 1`): per-scan LZF blobs with
    signed-delta TOF stream (pure-Rust LZF; no `liblzf` dependency).
- TOF -> m/z and scan -> 1/K0 calibration via the open-source
  linear-in-sqrt(m/z) model (< 2 ppm against vendor on the probe corpus).
- Acquisition-mode metadata:
  - diaPASEF windows (`DiaFrameMsMsInfo` + `DiaFrameMsMsWindows`)
  - PASEF DDA precursors + per-frame MS/MS info
  - prm-PASEF targets + per-frame MS/MS info
- Schema-version compatibility: 3.1, 3.3, 3.5, 3.6, 3.7 verified.
- `examples/dump.rs`: minimal frame-peak dumper for CLI inspection.
- 9 integration tests, of which 8 run against a small committed probe
  corpus and 1 (PRM) is conditional on the optional PRIDE PXD028279
  probe being present.

### Out of scope

- `analysis.tsf` (MALDI / non-TIMS bundles).
- Tune-method XML blocks under `*.m/`.
- Proprietary Bruker polynomial calibration models (the linear model is
  used; see `docs/format/04-tof-to-mz-calibration.md`).
