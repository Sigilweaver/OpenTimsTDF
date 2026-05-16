---
sidebar_position: 2
---

# Install

OpenTDF is published on [crates.io](https://crates.io/crates/opentdf).

## As a library dependency

```toml
# Cargo.toml
[dependencies]
opentdf = "0.1"
```

## From source

```sh
git clone https://github.com/Sigilweaver/OpenTDF
cd OpenTDF
cargo build --release
```

## Requirements

- Rust 1.75 or later (set as `rust-version` in `Cargo.toml`).
- A C toolchain is required transitively for `rusqlite` (bundled SQLite)
  and `zstd`. On Linux this is usually `build-essential`; on macOS, Xcode
  Command Line Tools; on Windows, the MSVC build tools.

No Bruker SDK, no `liblzf`, no system SQLite is required: all native
code is vendored through cargo.

## Verifying the install

```sh
cargo test
```

The full suite is nine tests; corpus-gated tests that require local
Bruker bundles skip silently when data is not present.
