---
sidebar_position: 4
---

# Peaks and codecs

`decode_peaks` is the only entry point you should need; the codec
selection is automatic. This page documents what happens under the
hood for users who need to interpret performance or implement a
compatible reader.

## Output shape

```rust
pub struct Peak {
    pub scan: u32,       // mobility bin
    pub tof: u32,        // TOF index
    pub intensity: u32,  // detector counts
}
```

The output is a flat `Vec<Peak>` for a frame. Peaks are emitted in
`(scan, tof)` order. Bruker's vendor SDK exposes the same data shape;
the byte-level decoding is what differs.

## Codec dispatch

`compression_type()` returns the `GlobalMetadata` `TimsCompressionType`
value, which determines the decode path used by `decode_peaks`:

| Value | Codec | Pipeline |
| ----- | ----- | -------- |
| 1     | LZF + signed-delta TOF | One LZF blob per scan, signed-delta-encoded TOF stream. Pure-Rust LZF decoder; no `liblzf` dependency. |
| 2     | zstd + byte-transpose + delta | Single zstd blob per frame, untransposed back into TOF and intensity streams, then delta-decoded. |

For the exact byte layout and the LZF implementation specifics, see
the format spec:

- [02-tdf-bin-block-stream.md](../format/tdf-bin-block-stream)
- [03-frame-payload-encoding.md](../format/frame-payload-encoding)
