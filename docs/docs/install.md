---
sidebar_position: 2
---

# Install

OpenTimsTDF ships as a Rust crate and a Python package.

## Rust

Published on [crates.io](https://crates.io/crates/opentimstdf).

```toml
# Cargo.toml
[dependencies]
opentimstdf = "3"
```

From source:

```sh
git clone https://github.com/Sigilweaver/OpenTimsTDF
cd OpenTimsTDF
cargo build --release
```

### Requirements

- Rust 1.75 or later.
- A C toolchain is required transitively for `rusqlite` (bundled SQLite)
  and `zstd`: `build-essential` on Linux, Xcode Command Line Tools on
  macOS, or MSVC build tools on Windows.

All native dependencies (SQLite, zstd, LZF) are vendored through cargo.

## Python

Published as [`opentimstdf`](https://pypi.org/project/opentimstdf/) on PyPI.

```sh
pip install opentimstdf
```

Built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs);
wheels target Python 3.9+.

From source (requires a working Rust toolchain and `maturin`):

```sh
git clone https://github.com/Sigilweaver/OpenTimsTDF
cd OpenTimsTDF/python
maturin develop --release
```

## Verifying the install

Rust:

```sh
cargo test
```

Python:

```sh
python -c "import opentimstdf; print(opentimstdf.__version__)"
```

Corpus-gated tests that need local `.d/` bundles print a skip message
and pass when no sample data is present. Set `OPENTIMSTDF_TEST_BUNDLE`
to a bundle and `REQUIRE_CORPUS=1` to make a missing bundle a failure
(`REQUIRE_CORPUS=all` also requires the PRIDE bundles under
`OPENTIMSTDF_TEST_CACHE`).
