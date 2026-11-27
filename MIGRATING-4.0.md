# Migrating to opentimstdf 4.0

4.0 adopts `openmassspec-core` 4.0. The reader's own API (`Reader`,
`Frame`, `Calibration`, `Peak`) is unchanged. What changes is the canonical
record that `TdfSource` yields and the Python dicts built from it. Core's
own list of breaks is in
[MIGRATING-4.0.md](https://github.com/Sigilweaver/OpenMassSpecCore/blob/main/MIGRATING-4.0.md)
in OpenMassSpecCore; the full list for this crate is in
[CHANGELOG.md](CHANGELOG.md).

## Dependency

```toml
[dependencies]
opentimstdf = "4"
openmassspec-core = "4"   # if you use TdfSource or the core writer directly
```

Python needs 3.10 or newer.

## Rust: records

| 3.0 | 4.0 |
| --- | --- |
| `precursor.collision_energy: Option<f64>` | `precursor.collision_energies: Vec<CollisionEnergy>`; each has `value` and `unit: Option<CollisionEnergyUnit>` |
| `precursor.ce_is_nce: bool` | `unit == Some(CollisionEnergyUnit::Nce)`; this reader always writes `Some(CollisionEnergyUnit::Ev)` |
| `record.retention_time_sec: f64` | `Option<f64>` (always `Some` from this reader) |
| `record.scan_window` | `record.scan_windows: Vec<ScanWindow>` (one entry, the bundle's stated m/z acquisition range) |

Match on Core enums with a `_` arm. `iter_spectra` and `iter_chromatograms`
return `Send` iterators.

## Python: record dicts

`Reader.iter_records()` and `Reader.run_info()` build their dicts from Core's
serialised records, so the keys follow Core 4:

| 3.0 | 4.0 |
| --- | --- |
| `record["precursor"]["collision_energy"]` (float) | `record["precursor"]["collision_energies"]`, a list of `{"value": float, "unit": "ev"}` |
| `record["precursor"]["ce_is_nce"]` | removed; read `unit` |
| `record["analyzer"] == "tofms"` | `"tof"` (also `record["precursor"]["analyzer"]`) |
| `record["retention_time_sec"]` always a float | may be `None` in Core 4; always a float from this reader |
| `base_peak_mz`, `base_peak_intensity`, `low_mz`, `high_mz` are `0.0` on a spectrum with no peaks | `None` |
| (none) | `record["warnings"]`, a list of `{"kind", "message"}`, present when non-empty |

Before and after, one PASEF MS2 record (peak arrays elided):

```python
# 3.0.1
{"analyzer": "tofms", "scan_mode": "centroid", "retention_time_sec": 241.07,
 "precursor": {"analyzer": "tofms", "collision_energy": 36.70, "ce_is_nce": False, ...}, ...}
# 4.0.0
{"analyzer": "tof", "scan_mode": "centroid", "retention_time_sec": 241.07,
 "precursor": {"analyzer": "tof",
               "collision_energies": [{"value": 36.70, "unit": "ev"}], ...}, ...}
```

## mzML output

- `<scanWindowList>` holds the bundle's stated m/z acquisition range
  (`MzAcqRangeLower` to `MzAcqRangeUpper`). Core 3 wrote each spectrum's
  observed m/z range there, which is not the instrument's scan window. It
  does not bound the peaks: observed m/z can exceed it by up to 5.0 on either
  side (see Known issues in the README for the public counts).
- Spectra with no peaks carry the user parameter
  `openmassspec: warning empty_spectrum` and no base peak or observed m/z
  range (3.0 wrote a base peak and range of 0.0 and no flag).
- The run carries `openmassspec: warning opentimstdf.frames_not_projected`
  when frames (prm-PASEF among them) produced no spectrum; the message has
  the counts.
- The strict checks in Core's writer (unknown scan mode, collision energy
  with no unit, unsorted or NaN m/z, MS level 0, dangling precursor
  `spectrumRef`) apply. This reader states scan mode and unit, so only the
  dangling reference can occur; see Known issues in the README.
