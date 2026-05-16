---
sidebar_position: 2
---

# Install

OpenTDF ships as a Rust crate and a Python package.

## Rust

Published on [crates.io](https://crates.io/crates/opentdf).

```toml
# Cargo.toml
[dependencies]
opentdf = "0.1"
```

From source:

```sh
git clone https://github.com/Sigilweaver/OpenTDF
cd OpenTDF
cargo build --release
```

### Requirements

- Rust 1.75 or later.
- A C toolchain is required transitively for `rusqlite` (bundled SQLite)
  and `zstd`: `build-essential` on Linux, Xcode Command Line Tools on
  macOS, or MSVC build tools on Windows.

All native dependencies (SQLite, zstd, LZF) are vendored through cargo.

## Python

Published as [`opentdf`](https://pypi.org/project/opentdf/) on PyPI.

```sh
pip install opentdf
```

Built with [PyO3](https://pyo3.rs) and [maturin](https://www.maturin.rs);
wheels target Python 3.9+.

From source (requires a working Rust toolchain and `maturin`):

```sh
git clone https://github.com/Sigilweaver/OpenTDF
cd OpenTDF/python
maturin develop --release
```

## Verifying the install

Rust:

```sh
cargo test
```

Python:

```sh
python -c "import opentdf; print(opentdf.__version__)"
```

Corpus-gated tests that need local `.d/` bundles skip silently when
no sample data is present.
