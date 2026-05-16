---
sidebar_position: 3
---

# Acquisition modes

Bruker timsTOF instruments emit a small set of acquisition modes.
OpenTDF identifies them through the `Frames.ScanMode` / `MsMsType`
columns and exposes the mode-specific metadata via dedicated reader
methods.

| Mode               | Detection                          | Reader entry point |
| ------------------ | ---------------------------------- | ------------------ |
| MS1 only           | `MsMsType = 0`                     | `reader.frame(id)` |
| PASEF DDA          | `MsMsType = 8`                     | `reader.pasef_msms_info_for_frame(id)` + `reader.precursor(id)` |
| diaPASEF           | `MsMsType = 9`                     | `reader.dia_windows_for_frame(id)` |
| prm-PASEF          | `MsMsType = 10`                    | `reader.prm_msms_info_for_frame(id)` + `reader.prm_target(id)` |

The bundle is not constrained to a single mode: prm-PASEF and diaPASEF
runs both interleave MS1 frames with mode-specific MS2 frames.

## diaPASEF

```rust
if let Some(fw) = reader.dia_windows_for_frame(frame.id)? {
    for w in &fw.windows {
        println!(
            "scan_begin={:>4} scan_end={:>4} mz={:.2} width={:.1} ce={:.1}",
            w.scan_num_begin, w.scan_num_end,
            w.isolation_mz, w.isolation_width, w.collision_energy,
        );
    }
}
```

## PASEF DDA

```rust
for info in reader.pasef_msms_info_for_frame(frame.id)? {
    let prec = reader.precursor(info.precursor_id)?.unwrap();
    println!(
        "precursor {} mz={:.4} z={:?} parent_frame={}",
        prec.id, prec.mono_mz, prec.charge, prec.parent_frame_id,
    );
}
```

## prm-PASEF

```rust
for entry in reader.prm_msms_info_for_frame(frame.id)? {
    if let Some(target) = reader.prm_target(entry.target_id)? {
        println!(
            "target id={} external={} mono_mz={:.4} charge={}",
            target.id, target.external_id, target.monoisotopic_mz, target.charge,
        );
    }
}
```
