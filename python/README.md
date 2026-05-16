# opentdf (Python bindings)

Python bindings for the [OpenTDF](https://github.com/Sigilweaver/OpenTDF)
Rust crate.

```sh
pip install opentdf
```

```python
import opentdf

reader = opentdf.Reader("my_bundle.d")
calib = reader.calibration()
frame = reader.frame(1)
for peak in reader.decode_peaks(frame):
    mz = calib.tof_to_mz(peak.tof)
    print(peak.scan, mz, peak.intensity)
```

See the [docs site](https://sigilweaver.app/opentdf/docs) for the full API.

## Build from source

Requires Rust 1.75+ and [maturin](https://www.maturin.rs):

```sh
cd python
maturin develop --release
```

## License

Apache-2.0.
