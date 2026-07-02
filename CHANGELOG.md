# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.2.0] - 2026-07-02

### Added

- `Metadata.acquisition_date_time` (Rust + Python): acquisition start
  timestamp read from `GlobalMetadata.AcquisitionDateTime`. Returns
  `None` when the key is absent from the bundle.
- `Frame.polarity` (Python): ion polarity derived from
  `mz_calibration_id` - `"positive"` for id 1, `"negative"` for id 2.
  Useful for dual-polarity acquisitions where frames alternate ids.
- `DecodedSpectrum` class and `Reader.decode_spectrum(frame)` (Python):
  decodes peaks and applies calibration in a single lock acquisition,
  returning parallel `mz`, `inv_mobility`, and `intensity` arrays.
  Arrays are plain Python lists; convert to numpy with `np.array()`.

### Changed

- `publish.yml`: crates.io publish step uses `continue-on-error: true`
  so re-triggered tag runs do not fail the workflow when the crate
  version was already published.

## [1.1.0] - 2026-05-31

### Added

- `CITATION.cff`: author identity (Nathan Riley + ORCID) and a
  scaffolded `identifiers:` block ready for the Zenodo concept DOI.
- `CONTRIBUTING.md`.
- Docusaurus build job in CI.

### Changed

- **Panic surface eliminated (WP17).** Reader mutex locks now
  `map_err(...)` into `CorruptFrame`-style errors with a "mutex
  poisoned" message, PyO3 mutex locks raise `PyRuntimeError`, and
  `chunks_exact(4)` retains a localized `#[allow]`. Library crate
  carries `#![cfg_attr(not(test), warn(clippy::unwrap_used,
  clippy::expect_used))]`.
- Project renamed from OpenTDF to OpenTimsTDF; documentation,
  scripts, and PyPI metadata updated accordingly.
- Manifest hygiene (WP13): `homepage` set to <https://sigilweaver.app>
  and `documentation` link added.
- README badge block unified across the Sigilweaver portfolio.

## [1.0.6] - 2026-05-21

### Changed

- Depend on `openproteo-core = "1.0.0"` (was `0.1.0`, yanked).
- MSRV bumped from 1.75 to 1.85 (tracks `openproteo-core 1.0.0`).

## [1.0.5] - 2026-05-18

### Changed

- Depend on `openproteo-core = "0.1.0"` from crates.io (workspace
  dependency now carries an explicit registry version so the crate can
  be published).
- `SECURITY.md` added; coordinated-disclosure contact documented.

## [1.0.4] - 2026-05-17

### Changed

- Restructured to a Cargo workspace layout. The library crate is now at
  `crates/opentimstdf/` and the Python bindings crate at
  `crates/opentimstdf-py/`. The `pyproject.toml` is now at the repository
  root. No public API changes.

## [1.0.3] - 2026-05-17

### Fixed

- `python/pyproject.toml`: revert `readme` to `"README.md"` and restore
  `python/README.md` stub. Maturin sdist packaging prohibits `..` in
  archive paths, causing the 1.0.2 sdist build to fail on CI.

## [1.0.2] - 2026-05-17

### Changed

- Docs and source comments: replace em-dashes and en-dashes with ASCII
  hyphens for consistent rendering across editors and terminals.

## [1.0.1] - 2026-05-17

### Changed

- README: standardize structure and docs link format (consistent with
  OpenTFRaw and OpenWRaw).
- Docs: rename all `OpenTimsTDF` references to `opentimstdf` throughout the
  Docusaurus source pages.

## [1.0.0] - 2026-05-17

First stable release under the new name `opentimstdf` (renamed from
`OpenTimsTDF` to avoid collision with the unrelated OpenTimsTDF organization on
GitHub and the Trusted Data Format ecosystem). The public API of
`opentimstdf` is now considered stable and will follow semantic
versioning. The schema-version compatibility set (TDF 3.1, 3.3, 3.5,
3.6, 3.7) is unchanged from `OpenTimsTDF` 0.1.1.

The crate, Python package, and GitHub repository have all been renamed:

- crates.io: `OpenTimsTDF` -> `opentimstdf`
- PyPI: `OpenTimsTDF` -> `opentimstdf`
- GitHub: `Sigilweaver/OpenTimsTDF` -> `Sigilweaver/OpenTimsTDF`
- Python module: `import OpenTimsTDF` -> `import opentimstdf`
- Rust crate: `use OpenTimsTDF::Reader` -> `use opentimstdf::Reader`

### Added

- `publish.yml` GitHub Actions workflow: publishes the `opentimstdf` crate to
  crates.io and the Python wheel to PyPI via OIDC Trusted Publishing on
  every `v*` tag push.

### Fixed

- Removed needless borrows in `Reader::open()` calls in integration tests
  (resolves `clippy::needless_borrows_for_generic_args`).

### Changed

- CI migrated from WarpBuild runners to standard GitHub-hosted
  (`ubuntu-latest`, `macos-latest`, `windows-latest`).

## [0.1.1] - 2026-05-16

Initial public release.

### Added

- Rust reader for timsTOF `.d/` (TDF) bundles.
- SQLite metadata access via bundled `rusqlite`: `GlobalMetadata`,
  `Frames`, mode-specific index tables, and calibration tables.
- Binary frame decoder supporting both compression codecs:
  - Codec 2 (`TimsCompressionType = 2`): zstd + byte-transpose + delta.
  - Codec 1 (`TimsCompressionType = 1`): per-scan LZF blobs with
    signed-delta TOF stream (Rust LZF decoder; no `liblzf` dependency).
- TOF -> m/z and scan -> 1/K0 calibration via the
  linear-in-sqrt(m/z) model.
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
- Proprietary polynomial calibration models (the linear model is
  used; see `docs/format/04-tof-to-mz-calibration.md`).

[1.0.1]: https://github.com/Sigilweaver/OpenTimsTDF/releases/tag/v1.0.1
[1.0.0]: https://github.com/Sigilweaver/OpenTimsTDF/releases/tag/v1.0.0
[0.1.1]: https://github.com/Sigilweaver/OpenTimsTDF/releases/tag/v0.1.1
