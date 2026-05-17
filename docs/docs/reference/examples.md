---
sidebar_position: 2
---

# Examples

The OpenTimsTDF repository ships runnable examples in
[`examples/`](https://github.com/Sigilweaver/OpenTimsTDF/tree/main/examples).

## `dump`

Print all peaks of a single frame.

```sh
cargo run --release --example dump -- path/to/bundle.d 1
```

Source: [`examples/dump.rs`](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/examples/dump.rs).

## Tests as worked examples

The integration tests under
[`tests/roundtrip.rs`](https://github.com/Sigilweaver/OpenTimsTDF/blob/main/tests/roundtrip.rs)
exercise every public method against the bundled probe corpus. They are
the most authoritative end-to-end demonstration of the API.
