---
sidebar_position: 2
---

# Python API

`opentimstdf.Reader` opens a `.d/` (TDF) bundle once and serves per-frame
metadata, calibration, and decoded peaks. The supporting classes below are
plain data carriers returned by `Reader`'s methods; none of them are
constructed directly.

```python
import opentimstdf

reader = opentimstdf.Reader("run.d")
```

## Reader

| Member | Returns | Description |
| --- | --- | --- |
| `bundle_dir` | `str` | Path to the opened `.d/` directory |
| `compression_type` | `int` | Codec used for peak blocks (1 or 2) |
| `metadata()` | `Metadata` | Instrument, software, and schema-version metadata |
| `calibration()` | `Calibration` | m/z and inverse-mobility calibration for the run |
| `frame(id)` | `Frame` | Metadata for a single frame by id |
| `frames()` | `list[Frame]` | Metadata for every frame in the run, in id order |
| `decode_peaks(frame)` | `list[Peak]` | Raw (uncalibrated) peaks for a frame |
| `decode_spectrum(frame)` | `DecodedSpectrum` | Calibrated peaks for a frame, as numpy arrays |
| `dia_windows_for_frame(frame_id)` | `DiaFrameWindows \| None` | diaPASEF isolation windows for an MS2 frame |
| `pasef_msms_info_for_frame(frame_id)` | `list[PasefMsMsInfo]` | PASEF precursor selections for an MS2 frame |
| `prm_msms_info_for_frame(frame_id)` | `list[PrmMsMsInfo]` | PRM target selections for an MS2 frame |
| `prm_target(target_id)` | `PrmTarget \| None` | A single PRM target definition by id |
| `precursor(precursor_id)` | `Precursor \| None` | A single DDA precursor by id |

```python
frame = reader.frame(1)
for peak in reader.decode_peaks(frame):
    ...  # Peak(scan, tof, intensity), uncalibrated

spectrum = reader.decode_spectrum(frame)
spectrum.mz            # numpy.ndarray, dtype=float64
spectrum.inv_mobility   # numpy.ndarray, dtype=float64
spectrum.intensity      # numpy.ndarray, dtype=uint32
```

`decode_peaks` and `decode_spectrum` both do the same lock-acquire-and-decode
work; `decode_spectrum` additionally applies calibration in the same pass,
so prefer it over pairing `decode_peaks` with manual `Calibration` calls
when you need m/z and inverse mobility rather than raw scan/tof.

## Supporting classes

### Peak

Raw, uncalibrated per-ion values decoded straight from a frame's block
stream.

| Field | Type |
| --- | --- |
| `scan` | `int` |
| `tof` | `int` |
| `intensity` | `int` |

### Frame

| Field | Type | Description |
| --- | --- | --- |
| `id` | `int` | Frame id |
| `time` | `float` | Retention time (seconds) |
| `num_scans` | `int` | Number of mobility scans in the frame |
| `num_peaks` | `int` | Total peak count |
| `tims_id` | `int` | Offset of this frame's block in `analysis.tdf_bin` |
| `scan_mode` | `int` | Acquisition scan mode |
| `msms_type` | `int` | 0 for MS1, otherwise identifies the MS2 acquisition mode |
| `mz_calibration_id` | `int` | Calibration id used for this frame |
| `accumulation_time` | `float \| None` | TIMS accumulation time (ms) |
| `summed_intensities` | `int \| None` | Sum of all peak intensities in the frame |
| `polarity` | `str` | `"positive"` or `"negative"`, derived from `mz_calibration_id` |

### Metadata

| Field | Type |
| --- | --- |
| `schema_version_major` | `int` |
| `schema_version_minor` | `int` |
| `instrument_name` | `str` |
| `acquisition_software` | `str` |
| `acquisition_software_version` | `str` |
| `compression_type` | `int` |
| `acquisition_date_time` | `str \| None` |

### Calibration

Converts between raw TOF/scan indices and calibrated m/z / inverse
mobility.

| Member | Type | Description |
| --- | --- | --- |
| `mz_intercept` | `float` | m/z calibration intercept |
| `mz_slope` | `float` | m/z calibration slope |
| `im_intercept` | `float` | Inverse-mobility calibration intercept |
| `im_slope` | `float` | Inverse-mobility calibration slope |
| `tof_to_mz(tof)` | `float` | Calibrated m/z for a raw TOF index |
| `mz_to_tof(mz)` | `int` | Raw TOF index nearest a given m/z |
| `scan_to_inv_mobility(scan)` | `float` | Calibrated 1/K0 for a raw scan index |
| `inv_mobility_to_scan(inv_mobility)` | `int` | Raw scan index nearest a given 1/K0 |

### DiaWindow / DiaFrameWindows

`dia_windows_for_frame` returns one `DiaFrameWindows` (or `None` for
non-diaPASEF frames), which fans out to the individual isolation windows
in its `windows` list.

**DiaFrameWindows**

| Field | Type |
| --- | --- |
| `frame_id` | `int` |
| `window_group` | `int` |
| `windows` | `list[DiaWindow]` |

**DiaWindow**

| Field | Type |
| --- | --- |
| `window_group` | `int` |
| `scan_num_begin` | `int` |
| `scan_num_end` | `int` |
| `isolation_mz` | `float` |
| `isolation_width` | `float` |
| `collision_energy` | `float` |

### PasefMsMsInfo

| Field | Type |
| --- | --- |
| `frame_id` | `int` |
| `scan_num_begin` | `int` |
| `scan_num_end` | `int` |
| `isolation_mz` | `float` |
| `isolation_width` | `float` |
| `collision_energy` | `float` |
| `precursor_id` | `int` (pass to `Reader.precursor()`) |

### PrmMsMsInfo / PrmTarget

`prm_msms_info_for_frame` returns `PrmMsMsInfo` entries; each carries a
`target_id` to look up the full target definition via
`Reader.prm_target()`.

**PrmMsMsInfo**

| Field | Type |
| --- | --- |
| `frame_id` | `int` |
| `scan_num_begin` | `int` |
| `scan_num_end` | `int` |
| `isolation_mz` | `float` |
| `isolation_width` | `float` |
| `collision_energy` | `float` |
| `target_id` | `int` (pass to `Reader.prm_target()`) |

**PrmTarget**

| Field | Type |
| --- | --- |
| `id` | `int` |
| `external_id` | `str` |
| `time` | `float` |
| `one_over_k0` | `float` |
| `monoisotopic_mz` | `float` |
| `charge` | `int` |
| `description` | `str` |

### Precursor

A DDA precursor selection, looked up via `Reader.precursor()` using the
`precursor_id` from a `PasefMsMsInfo`.

| Field | Type |
| --- | --- |
| `id` | `int` |
| `largest_peak_mz` | `float` |
| `average_mz` | `float` |
| `monoisotopic_mz` | `float \| None` |
| `charge` | `int \| None` |
| `scan_number` | `float` |
| `intensity` | `float` |
| `parent_frame_id` | `int` |

### DecodedSpectrum

Returned by `Reader.decode_spectrum()`. All three arrays are the same
length; index `i` describes one calibrated ion.

| Field | Type | Description |
| --- | --- | --- |
| `mz` | `numpy.ndarray[float64]` | Calibrated m/z values (Da) |
| `inv_mobility` | `numpy.ndarray[float64]` | Calibrated inverse ion mobility (1/K0, V*s/cm^2) |
| `intensity` | `numpy.ndarray[uint32]` | Raw intensity counts |

`len(spectrum)` returns the peak count.

## Next

- [API reference](./api) (Rust, on docs.rs)
- [Reader guide](../guide/reader)
