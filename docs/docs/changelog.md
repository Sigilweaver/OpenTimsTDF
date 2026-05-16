---
sidebar_position: 90
---

# Changelog

The authoritative changelog lives in the repository:

- [CHANGELOG.md on GitHub](https://github.com/Sigilweaver/OpenTDF/blob/main/CHANGELOG.md)
- [Releases](https://github.com/Sigilweaver/OpenTDF/releases)

## Current release

### 0.1.1 - 2026-05-16

Initial public release. Covers:

- SQLite metadata access via bundled `rusqlite`.
- Codec 1 (LZF + signed-delta) and Codec 2 (zstd + byte-transpose + delta)
  frame decoders.
- TOF to m/z and scan to 1/K0 calibration using the
  linear-in-sqrt(m/z) model.
- diaPASEF, PASEF DDA, and prm-PASEF metadata helpers.
- Schema versions 3.1, 3.3, 3.5, 3.6, 3.7.

See [CHANGELOG.md](https://github.com/Sigilweaver/OpenTDF/blob/main/CHANGELOG.md)
for the full entry.
