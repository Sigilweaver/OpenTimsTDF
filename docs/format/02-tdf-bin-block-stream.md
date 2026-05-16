# `analysis.tdf_bin` - block stream

The companion file `analysis.tdf_bin` is a sequence of per-frame
**blocks**, indexed exclusively by `Frames.TimsId` (see
[01-tdf-sqlite-schema.md](01-tdf-sqlite-schema.md#frames---authoritative-frame-index)).

Blocks are **not guaranteed to be contiguous**. On real-world bundles
written by timsControl we observe padding between blocks such that
the start of each frame is aligned to an instrument-chosen stride
(observed: 4096, 12288, 16384, 20480 B).

Readers MUST `seek(TimsId)`. They MUST NOT attempt
`offset += block_size` traversal.

## Block header

Every block begins with 8 bytes:

```
offset  size  type   name
0       4     u32    block_size   -- total block size in bytes, including these 8
4       4     u32    scan_count   -- MUST equal Frames.NumScans for this row
```

## Block payload

- If `block_size == 8` then the frame is **empty** (no peaks). No
  payload bytes follow. `NumScans` still matches.
- If `block_size > 8` then bytes `[8, block_size)` are the **frame
  payload**, whose encoding is selected by `TimsCompressionType`
  (see [03-frame-payload-encoding.md](03-frame-payload-encoding.md)).

## Padding

Bytes between the end of one block (`TimsId + block_size`) and the
start of the next (next row's `TimsId`) are undefined / zero.

## File tail

The last block ends at `TimsId_max + block_size_max`. Trailing bytes
after that (up to a final stride boundary) may be present. Observed
utilisation `total_block_bytes / tdf_bin_size` ranges from **0.754**
(worst observed: PXD035147, sv 3.1) to **1.000**.
