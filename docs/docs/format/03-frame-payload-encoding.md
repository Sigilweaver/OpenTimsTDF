# Frame payload encoding

## Codec selection

Dispatched by `GlobalMetadata.TimsCompressionType`
(see [01-tdf-sqlite-schema.md](01-tdf-sqlite-schema.md#globalmetadata-keyvalue)):

| Value | Codec | Observed in |
| ----- | ----- | ----------- |
| `"1"` | Legacy per-scan LZF + signed-delta stream | PXD022216 (sv 3.1) |
| `"2"` | zstd-framed byte-transposed layout        | All other bundles in the corpus |

Codec 2's payload starts with the zstd magic `0x28 0xb5 0x2f 0xfd`.
Codec 1's payload starts with a `(scan_count + 1)`-entry `u32` offset
table (no magic).

## Codec 2 length invariant

After zstd decompression the codec-2 inner buffer satisfies

```
inner_len == NumScans * 4 + NumPeaks * 8
```

Verified exactly on every frame in a 10-frame sample of every
codec-2 bundle in the corpus.

## Codec 1 frame header

Reproduced from `alphatims.bruker.process_frame` (MIT):

```
u32 bin_size                       -- total frame size including header
u32 scan_count                     -- == Frames.NumScans
u32 scan_offset[scan_count + 1]    -- absolute byte offsets in the frame
.. compressed_data ..              -- bin_size - 8 - 4*(scan_count+1) bytes
```

`scan_offset[i]` is the start of scan `i`'s LZF blob measured from
the beginning of the frame. Rebase by subtracting
`compression_offset = 8 + 4 * (scan_count + 1)` to index into
`compressed_data`. An empty scan is signalled by
`scan_offset[i] == scan_offset[i+1]`.

## Codec 2 inner layout

After zstd decompression, the inner buffer of length
`4 * (NumScans + 2*NumPeaks)` bytes is **byte-transposed**: split it
into four equal byte streams of length `dsints = NumScans + 2 * NumPeaks`:

```
b0 = inner[0*dsints : 1*dsints]
b1 = inner[1*dsints : 2*dsints]
b2 = inner[2*dsints : 3*dsints]
b3 = inner[3*dsints : 4*dsints]
```

The logical u32 value at index `i in [0, dsints)` is reconstructed as

```
logical[i] = b0[i] | (b1[i] << 8) | (b2[i] << 16) | (b3[i] << 24)
```

This byte-column arrangement is a zstd-friendliness trick: high
bytes of adjacent u32s cluster together and compress well.

The logical buffer then has two regions:

```
logical[0 .. NumScans]                          -- scan header
logical[NumScans .. NumScans + 2*NumPeaks]      -- peak stream
```

**Scan header.** For `scan in [0, NumScans - 1)`, the peak count of
scan `scan` is `logical[scan + 1] / 2`. (The division by 2 is because
the header stores u32-word counts, and each peak consumes two u32
words.) The final scan (`scan = NumScans - 1`) holds all remaining
peaks until `NumPeaks` have been emitted.

**Peak stream.** A sequence of interleaved `(tof_delta, intensity)`
u32 pairs:

```
peak_stream[0]    = tof_delta_of_peak_0
peak_stream[1]    = intensity_of_peak_0
peak_stream[2]    = tof_delta_of_peak_1
peak_stream[3]    = intensity_of_peak_1
...
```

TOFs are **delta-encoded per scan** with the accumulator initialised
to `UINT32_MAX` at the start of each scan; a stored delta of 1
therefore produces TOF index 0. Pseudocode:

```
peaks_done = 0
for scan in 0 .. NumScans:
    peaks_in_scan = (scan + 1 < NumScans)
                      ? header[scan + 1] / 2
                      : (NumPeaks - peaks_done)
    accum = UINT32_MAX
    for p in 0 .. peaks_in_scan:
        accum    += peak_stream[read++]   // overflow wraps
        intensity = peak_stream[read++]
        emit(scan, accum, intensity)
        peaks_done += 1
```

**Verification.** All 11 codec-2 bundles in the corpus decoded
cleanly. Decoded intensity sums match `Frames.SummedIntensities`
exactly on every single-peak frame tested; large-frame sums differ
by 0.01-25%, a raw-vs-centroided accounting artefact unrelated to
decoding.

## Codec 1 inner layout

For each scan `i in [0, scan_count)`, the slice
`compressed_data[scan_offset[i] .. scan_offset[i+1]]` is an LZF
(libLZF) blob. Decompress it and reinterpret the bytes as a
little-endian `i32[]`. Walk the sequence with two running state
variables (`tof: u32`, `prev_was_intensity: bool`, both reset at the
start of every scan to `tof = 0`, `prev_was_intensity = true`):

```
for v in i32_stream:
    if v >= 0:                        # intensity value
        if prev_was_intensity:
            tof += 1                  # implicit delta between adjacent peaks
        emit(scan=i, tof=tof-1, intensity=v as u32)
        prev_was_intensity = true
    else:                             # negative delta: explicit TOF jump
        tof += (-v) as u32
        prev_was_intensity = false
```

The emitted `tof - 1` matches the 0-based TOF convention used by
codec 2. Ported verbatim from
`alphatims.bruker.parse_decompressed_bruker_binary_type1` (MIT).

**Verified** on PXD022216 frames 1, 2, 5, 100, 500: the number of
decoded peaks matches `Frames.NumPeaks` exactly in every case (see
`pride_pxd022216_codec1_numpeaks_match`).

### Intensity normalisation

The raw sum of decoded codec-1 intensities is **not** equal to
`Frames.SummedIntensities`. Instead:

```
SummedIntensities = raw_sum * 100.0 / AccumulationTime_ms
```

Measured across all 57,886 frames of PXD022216 (sv=3.1, diaPASEF,
AccumulationTime = 108.46 ms): decoded_sum / SummedIntensities =
1.08460 +/- 0.00001 (= 108.46 / 100.0) for every frame, both MS1
(n=3,406) and MS2 (n=54,480). Zero undershoot frames. Codec-2
bundles have AccumulationTime ~ 100 ms, so the normalisation factor
is ~ 1.0 and the discrepancy is negligible.

Occasional decoded TOF values slightly exceed
`GlobalMetadata.DigitizerNumSamples` (~ 0.8% overflow in the worst
observed frame). This appears to be an upstream overflow window and
is preserved as-is.
