---
sidebar_position: 3
---

# Quickstart

A minimal end-to-end read of a Bruker `.d/` bundle.

```rust
use opentdf::Reader;

fn main() -> opentdf::Result<()> {
    let reader = Reader::open("my_bundle.d")?;
    let calib = reader.calibration()?;

    let frame = reader.frame(1)?;
    for peak in reader.decode_peaks(&frame)? {
        let mz = calib.tof_to_mz(peak.tof);
        let one_over_k0 = calib.scan_to_inv_mobility(peak.scan);
        println!(
            "scan={:>5} tof={:>6} intensity={:>6} mz={:.4} 1/K0={:.4}",
            peak.scan, peak.tof, peak.intensity, mz, one_over_k0,
        );
    }
    Ok(())
}
```

## Iterating all frames

```rust
let reader = Reader::open("my_bundle.d")?;
for frame in reader.frames()? {
    let peaks = reader.decode_peaks(&frame)?;
    // ...
}
```

## Acquisition-mode metadata

OpenTDF exposes the windowing tables that Bruker writes for the major
PASEF acquisition modes. See [Acquisition modes](./guide/acquisition-modes)
for the full surface area.

```rust
// diaPASEF isolation windows for a given frame
if let Some(fw) = reader.dia_windows_for_frame(frame.id)? {
    for w in &fw.windows {
        println!(
            "window: mz={:.2} width={:.1} ce={:.1}",
            w.isolation_mz, w.isolation_width, w.collision_energy,
        );
    }
}

// prm-PASEF targets
for entry in reader.prm_msms_info_for_frame(frame.id)? {
    let target = reader.prm_target(entry.target_id)?.unwrap();
    println!(
        "target: {} mz={:.4} z={}",
        target.external_id, target.monoisotopic_mz, target.charge,
    );
}
```

## CLI example

The repository ships a `dump` example that prints peaks for a single
frame:

```sh
cargo run --release --example dump -- path/to/bundle.d 1
```
