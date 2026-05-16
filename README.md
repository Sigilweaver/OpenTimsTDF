# OpenTDF

An open, pure-Rust parser for **Bruker timsTOF `.d/` (TDF)** mass
spectrometry bundles - reverse-engineered from scratch, without the
vendor SDK.

## Status

**v0.1.1 - all frame decoding paths, calibration, and acquisition-mode metadata
work end-to-end.** See [`re/`](re/) for the derivation log (`JOURNAL.md`) and
the running spec (`SPEC.md`).

What works today (verified against a 64-bundle probe corpus plus dedicated
prm-PASEF and diaPASEF bundles):

| Component | Status |
| --------- | ------ |
| `analysis.tdf` schema and metadata | verified (SQLite) |
| `analysis.tdf_bin` block layout | verified: `[u32 block_size | u32 scan_count | payload]` at `Frames.TimsId` |
| Codec 2 frame decode (zstd + byte-transpose + delta) | verified: intensity sum matches `Frames.SummedIntensities` |
| Codec 1 frame decode (LZF + signed-delta) | verified: `NumPeaks` matches on all sampled frames |
| TOF -> m/z calibration | open-source linear-in-sqrt(m/z) model (< 2 ppm) |
| Scan -> 1/K0 calibration | open-source linear model |
| DIA / diaPASEF window metadata | verified (PXD025576) |
| PASEF DDA MS2 metadata + precursors | verified (PXD027359) |
| prm-PASEF MS2 metadata + targets | verified (PXD028279) |
| Schema versions 3.1, 3.3, 3.5, 3.6, 3.7 | all confirmed; no codec differences |
| `analysis.tsf` (MALDI) | not in scope |

## Quick start

```rust
use opentdf::Reader;

let reader = Reader::open("my_bundle.d")?;
let calib = reader.calibration()?;         // TOF <-> m/z, scan <-> 1/K0
let frame = reader.frame(1)?;
for peak in reader.decode_peaks(&frame)? {
    let mz = calib.tof_to_mz(peak.tof);
    let one_over_k0 = calib.scan_to_inv_mobility(peak.scan);
    println!(
        "scan={} tof={} intensity={} mz={:.4} 1/K0={:.4}",
        peak.scan, peak.tof, peak.intensity, mz, one_over_k0,
    );
}
```

Query acquisition-mode metadata:

```rust
// diaPASEF isolation windows
if let Some(fw) = reader.dia_windows_for_frame(frame.id)? {
    for w in &fw.windows {
        println!("window: mz={:.2} width={:.1} ce={:.1}", w.isolation_mz, w.isolation_width, w.collision_energy);
    }
}

// prm-PASEF targets
for entry in reader.prm_msms_info_for_frame(frame.id)? {
    let target = reader.prm_target(entry.target_id)?.unwrap();
    println!("target: {} mz={:.4} z={}", target.external_id, target.monoisotopic_mz, target.charge);
}
```

Or via the dump example:

```
cargo run --example dump -- path/to/bundle.d 1
```

## Repository layout

```
OpenTDF/
├── src/
│   ├── lib.rs          # module declarations + pub use re-exports
│   ├── reader.rs       # Reader struct + all public API methods
│   ├── types.rs        # Frame, Peak, Metadata, DiaWindow, PasefMsMsInfo, PrmMsMsInfo, PrmTarget, ...
│   ├── calibration.rs  # Calibration struct + tof_to_mz / scan_to_inv_mobility
│   ├── codec.rs        # decode_codec1, decode_codec2, LZF decompressor
│   └── error.rs        # Error enum + Result<T> alias
├── examples/           # dump.rs
├── tests/              # roundtrip.rs (9 tests; corpus-gated tests skip silently without data)
├── corpus/
│   └── probes/         # Committed probe analysis.tdf files for always-on tests
└── re/                 # Reverse-engineering workspace
    ├── JOURNAL.md      # Daily log of findings, hypotheses, and experiments (18 entries)
    ├── SPEC.md         # Running format specification (~1100 lines)
    ├── corpus.md       # Bundle inventory + schema/codec matrix (64 probes)
    └── scripts/        # Python probe scripts
```

## References

- [alphatims](https://github.com/MannLabs/alphatims) (MIT) - reads TDF via the Bruker vendor SDK
- [opentims](https://github.com/michalsta/opentims) (BSD-2) - C++ open parser; basis for the open-source calibration model
- [rustims](https://github.com/theGreatHerrLebert/rustims) (MIT) - Rust TDF reader using the same linear calibration approximation

## License

Apache-2.0
