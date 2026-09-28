#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]
//! Python bindings for OpenTimsTDF.
//!
//! Exposes `opentimstdf.Reader`, which opens a `.d/` (TDF) bundle once and
//! serves per-frame metadata, calibration, and decoded peaks.

use std::path::PathBuf;
use std::sync::{
    mpsc::{sync_channel, Receiver},
    Mutex,
};

use numpy::{IntoPyArray, PyArray1, PyUntypedArrayMethods};
use openmassspec_core::{SpectrumRecord, SpectrumSource};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use ::opentimstdf::{
    Calibration as RsCalibration, DiaFrameWindows as RsDiaFrameWindows, DiaWindow as RsDiaWindow,
    Frame as RsFrame, Metadata as RsMetadata, PasefMsMsInfo as RsPasefMsMsInfo, Peak as RsPeak,
    Precursor as RsPrecursor, PrmMsMsInfo as RsPrmMsMsInfo, PrmTarget as RsPrmTarget,
    Reader as RsReader,
};

fn to_py_err(e: ::opentimstdf::Error) -> PyErr {
    PyRuntimeError::new_err(format!("{e}"))
}

/// Spectrum record as a dict. `total_ion_current` and `base_peak_*` carry the
/// effective values, matching `openmassspec_io.Spectrum`; the vendor-reported
/// values stay under `reported_*`. Peak arrays bypass JSON, which would turn
/// NaN into `None` and is slow for large spectra.
fn record_object(py: Python<'_>, mut rec: SpectrumRecord) -> PyResult<Py<PyAny>> {
    let tic = rec.effective_tic();
    let base_peak = rec.effective_base_peak();
    let mz = std::mem::take(&mut rec.mz);
    let intensity = std::mem::take(&mut rec.intensity);
    let mobility = rec.inv_mobility_per_peak.take();
    let obj = json_object(py, &rec)?;
    let d = obj.bind(py);
    d.set_item("mz", mz)?;
    d.set_item("intensity", intensity)?;
    d.set_item("inv_mobility_per_peak", mobility)?;
    d.set_item("total_ion_current", tic)?;
    d.set_item("base_peak_mz", base_peak.map(|p| p.0))?;
    d.set_item("base_peak_intensity", base_peak.map(|p| p.1))?;
    Ok(obj)
}

fn json_object<T: serde::Serialize>(py: Python<'_>, value: &T) -> PyResult<Py<PyAny>> {
    let mut value =
        serde_json::to_value(value).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    normalize_record_json(&mut value);
    let json = serde_json::to_string(&value).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    Ok(py.import("json")?.call_method1("loads", (json,))?.unbind())
}

fn normalize_record_json(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(fields) = value {
        if fields.contains_key("native_id") || fields.contains_key("source_file_name") {
            fields
                .entry("extra")
                .or_insert_with(|| serde_json::json!({}));
        }
        if fields.contains_key("native_id") {
            for (source, alias) in [
                ("total_ion_current", "reported_total_ion_current"),
                ("base_peak_mz", "reported_base_peak_mz"),
                ("base_peak_intensity", "reported_base_peak_intensity"),
            ] {
                if let Some(reported) = fields.get(source).cloned() {
                    fields.insert(alias.to_string(), reported);
                }
            }
        }
        for (key, field) in fields {
            match key.as_str() {
                "polarity" | "scan_mode" | "analyzer" | "activation" => {
                    if let Some(text) = field.as_str() {
                        *field = serde_json::Value::String(text.to_ascii_lowercase());
                    }
                }
                "mobility_array_kind" => {
                    if let Some(text) = field.as_str() {
                        let normalized = match text {
                            "InverseReducedVsPerCm2" => "inverse_reduced_k0",
                            "DriftTimeMilliseconds" => "drift_time_ms",
                            other => other,
                        };
                        *field = serde_json::Value::String(normalized.to_string());
                    }
                }
                "analyzers" => {
                    if let Some(items) = field.as_array_mut() {
                        for item in items {
                            if let Some(text) = item.as_str() {
                                *item = serde_json::Value::String(text.to_ascii_lowercase());
                            }
                        }
                    }
                }
                _ => normalize_record_json(field),
            }
        }
    }
}

/// Bounded stream of canonical spectrum records, decoded one frame at a time.
#[pyclass(module = "opentimstdf")]
struct RecordIter {
    // `None` marks a complete stream, so a closed channel without it means
    // the decode thread died.
    receiver: Mutex<Receiver<Option<SpectrumRecord>>>,
    finished: bool,
}

#[pymethods]
impl RecordIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        if slf.finished {
            return Ok(None);
        }
        let receiver = &slf.receiver;
        let message = py.detach(|| {
            receiver
                .lock()
                .map_err(|_| "record stream lock poisoned")?
                .recv()
                .map_err(|_| "record decode thread ended without a result")
        });
        match message {
            Ok(Some(rec)) => record_object(py, rec).map(Some),
            Ok(None) => {
                slf.finished = true;
                Ok(None)
            }
            Err(error) => {
                slf.finished = true;
                Err(PyRuntimeError::new_err(error))
            }
        }
    }
}

// -- Peak --------------------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "Peak", from_py_object)]
#[derive(Clone, Copy)]
struct Peak {
    #[pyo3(get)]
    scan: u32,
    #[pyo3(get)]
    tof: u32,
    #[pyo3(get)]
    intensity: u32,
}

#[pymethods]
impl Peak {
    fn __repr__(&self) -> String {
        format!(
            "Peak(scan={}, tof={}, intensity={})",
            self.scan, self.tof, self.intensity
        )
    }
}

impl From<RsPeak> for Peak {
    fn from(p: RsPeak) -> Self {
        Self {
            scan: p.scan,
            tof: p.tof,
            intensity: p.intensity,
        }
    }
}

// -- Frame -------------------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "Frame", from_py_object)]
#[derive(Clone)]
struct Frame {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    time: f64,
    #[pyo3(get)]
    num_scans: u32,
    #[pyo3(get)]
    num_peaks: u32,
    #[pyo3(get)]
    tims_id: u64,
    #[pyo3(get)]
    polarity_symbol: String,
    #[pyo3(get)]
    scan_mode: u32,
    #[pyo3(get)]
    msms_type: u32,
    #[pyo3(get)]
    mz_calibration_id: u32,
    #[pyo3(get)]
    accumulation_time: Option<f64>,
    #[pyo3(get)]
    summed_intensities: Option<u64>,
    #[pyo3(get)]
    max_intensity: Option<u64>,
}

#[pymethods]
impl Frame {
    /// Ion polarity read from `Frames.Polarity`.
    #[getter]
    fn polarity(&self) -> &'static str {
        match self.polarity_symbol.as_str() {
            "-" => "negative",
            "+" => "positive",
            _ => "unknown",
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Frame(id={}, time={:.3}, num_scans={}, num_peaks={}, msms_type={})",
            self.id, self.time, self.num_scans, self.num_peaks, self.msms_type
        )
    }
}

impl From<RsFrame> for Frame {
    fn from(f: RsFrame) -> Self {
        Self {
            id: f.id,
            time: f.time,
            num_scans: f.num_scans,
            num_peaks: f.num_peaks,
            tims_id: f.tims_id,
            polarity_symbol: f.polarity,
            scan_mode: f.scan_mode,
            msms_type: f.msms_type,
            mz_calibration_id: f.mz_calibration_id,
            accumulation_time: f.accumulation_time,
            summed_intensities: f.summed_intensities,
            max_intensity: f.max_intensity,
        }
    }
}

// -- Metadata ----------------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "Metadata", from_py_object)]
#[derive(Clone)]
struct Metadata {
    #[pyo3(get)]
    schema_version_major: u32,
    #[pyo3(get)]
    schema_version_minor: u32,
    #[pyo3(get)]
    instrument_name: String,
    #[pyo3(get)]
    acquisition_software: String,
    #[pyo3(get)]
    acquisition_software_version: String,
    #[pyo3(get)]
    compression_type: u32,
    #[pyo3(get)]
    acquisition_date_time: Option<String>,
}

#[pymethods]
impl Metadata {
    fn __repr__(&self) -> String {
        format!(
            "Metadata(schema={}.{}, instrument='{}', software='{} {}', codec={}, datetime={:?})",
            self.schema_version_major,
            self.schema_version_minor,
            self.instrument_name,
            self.acquisition_software,
            self.acquisition_software_version,
            self.compression_type,
            self.acquisition_date_time.as_deref().unwrap_or(""),
        )
    }
}

impl From<RsMetadata> for Metadata {
    fn from(m: RsMetadata) -> Self {
        Self {
            schema_version_major: m.schema_version_major,
            schema_version_minor: m.schema_version_minor,
            instrument_name: m.instrument_name,
            acquisition_software: m.acquisition_software,
            acquisition_software_version: m.acquisition_software_version,
            compression_type: m.compression_type,
            acquisition_date_time: m.acquisition_date_time,
        }
    }
}

// -- Calibration -------------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "Calibration", from_py_object)]
#[derive(Clone, Copy)]
struct Calibration {
    inner: RsCalibration,
}

#[pymethods]
impl Calibration {
    #[getter]
    fn mz_intercept(&self) -> f64 {
        self.inner.mz_intercept
    }

    #[getter]
    fn mz_slope(&self) -> f64 {
        self.inner.mz_slope
    }

    #[getter]
    fn im_intercept(&self) -> f64 {
        self.inner.im_intercept
    }

    #[getter]
    fn im_slope(&self) -> f64 {
        self.inner.im_slope
    }

    fn tof_to_mz(&self, tof: u32) -> f64 {
        self.inner.tof_to_mz(tof)
    }

    fn mz_to_tof(&self, mz: f64) -> u32 {
        self.inner.mz_to_tof(mz)
    }

    fn scan_to_inv_mobility(&self, scan: u32) -> f64 {
        self.inner.scan_to_inv_mobility(scan)
    }

    fn inv_mobility_to_scan(&self, inv_mobility: f64) -> u32 {
        self.inner.inv_mobility_to_scan(inv_mobility)
    }

    fn __repr__(&self) -> String {
        format!(
            "Calibration(mz=({:.6}+{:.6e}*tof)^2, 1/K0={:.4}+{:.4e}*scan)",
            self.inner.mz_intercept,
            self.inner.mz_slope,
            self.inner.im_intercept,
            self.inner.im_slope,
        )
    }
}

// -- DiaWindow / DiaFrameWindows --------------------------------------------

#[pyclass(module = "opentimstdf", name = "DiaWindow", from_py_object)]
#[derive(Clone)]
struct DiaWindow {
    #[pyo3(get)]
    window_group: u32,
    #[pyo3(get)]
    scan_num_begin: u32,
    #[pyo3(get)]
    scan_num_end: u32,
    #[pyo3(get)]
    isolation_mz: f64,
    #[pyo3(get)]
    isolation_width: f64,
    #[pyo3(get)]
    collision_energy: f64,
}

impl From<RsDiaWindow> for DiaWindow {
    fn from(w: RsDiaWindow) -> Self {
        Self {
            window_group: w.window_group,
            scan_num_begin: w.scan_num_begin,
            scan_num_end: w.scan_num_end,
            isolation_mz: w.isolation_mz,
            isolation_width: w.isolation_width,
            collision_energy: w.collision_energy,
        }
    }
}

#[pyclass(module = "opentimstdf", name = "DiaFrameWindows", from_py_object)]
#[derive(Clone)]
struct DiaFrameWindows {
    #[pyo3(get)]
    frame_id: u32,
    #[pyo3(get)]
    window_group: u32,
    #[pyo3(get)]
    windows: Vec<DiaWindow>,
}

impl From<RsDiaFrameWindows> for DiaFrameWindows {
    fn from(f: RsDiaFrameWindows) -> Self {
        Self {
            frame_id: f.frame_id,
            window_group: f.window_group,
            windows: f.windows.into_iter().map(Into::into).collect(),
        }
    }
}

// -- PasefMsMsInfo -----------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "PasefMsMsInfo", from_py_object)]
#[derive(Clone)]
struct PasefMsMsInfo {
    #[pyo3(get)]
    frame_id: u32,
    #[pyo3(get)]
    scan_num_begin: u32,
    #[pyo3(get)]
    scan_num_end: u32,
    #[pyo3(get)]
    isolation_mz: f64,
    #[pyo3(get)]
    isolation_width: f64,
    #[pyo3(get)]
    collision_energy: f64,
    #[pyo3(get)]
    precursor_id: u32,
}

impl From<RsPasefMsMsInfo> for PasefMsMsInfo {
    fn from(p: RsPasefMsMsInfo) -> Self {
        Self {
            frame_id: p.frame_id,
            scan_num_begin: p.scan_num_begin,
            scan_num_end: p.scan_num_end,
            isolation_mz: p.isolation_mz,
            isolation_width: p.isolation_width,
            collision_energy: p.collision_energy,
            precursor_id: p.precursor_id,
        }
    }
}

// -- PrmMsMsInfo / PrmTarget -------------------------------------------------

#[pyclass(module = "opentimstdf", name = "PrmMsMsInfo", from_py_object)]
#[derive(Clone)]
struct PrmMsMsInfo {
    #[pyo3(get)]
    frame_id: u32,
    #[pyo3(get)]
    scan_num_begin: u32,
    #[pyo3(get)]
    scan_num_end: u32,
    #[pyo3(get)]
    isolation_mz: f64,
    #[pyo3(get)]
    isolation_width: f64,
    #[pyo3(get)]
    collision_energy: f64,
    #[pyo3(get)]
    target_id: u32,
}

impl From<RsPrmMsMsInfo> for PrmMsMsInfo {
    fn from(p: RsPrmMsMsInfo) -> Self {
        Self {
            frame_id: p.frame_id,
            scan_num_begin: p.scan_num_begin,
            scan_num_end: p.scan_num_end,
            isolation_mz: p.isolation_mz,
            isolation_width: p.isolation_width,
            collision_energy: p.collision_energy,
            target_id: p.target_id,
        }
    }
}

#[pyclass(module = "opentimstdf", name = "PrmTarget", from_py_object)]
#[derive(Clone)]
struct PrmTarget {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    external_id: String,
    #[pyo3(get)]
    time: f64,
    #[pyo3(get)]
    one_over_k0: f64,
    #[pyo3(get)]
    monoisotopic_mz: f64,
    #[pyo3(get)]
    charge: u32,
    #[pyo3(get)]
    description: String,
}

impl From<RsPrmTarget> for PrmTarget {
    fn from(t: RsPrmTarget) -> Self {
        Self {
            id: t.id,
            external_id: t.external_id,
            time: t.time,
            one_over_k0: t.one_over_k0,
            monoisotopic_mz: t.monoisotopic_mz,
            charge: t.charge,
            description: t.description,
        }
    }
}

// -- Precursor ---------------------------------------------------------------

#[pyclass(module = "opentimstdf", name = "Precursor", from_py_object)]
#[derive(Clone)]
struct Precursor {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    largest_peak_mz: f64,
    #[pyo3(get)]
    average_mz: f64,
    #[pyo3(get)]
    monoisotopic_mz: Option<f64>,
    #[pyo3(get)]
    charge: Option<u32>,
    #[pyo3(get)]
    scan_number: f64,
    #[pyo3(get)]
    intensity: f64,
    #[pyo3(get)]
    parent_frame_id: u32,
}

impl From<RsPrecursor> for Precursor {
    fn from(p: RsPrecursor) -> Self {
        Self {
            id: p.id,
            largest_peak_mz: p.largest_peak_mz,
            average_mz: p.average_mz,
            monoisotopic_mz: p.monoisotopic_mz,
            charge: p.charge,
            scan_number: p.scan_number,
            intensity: p.intensity,
            parent_frame_id: p.parent_frame_id,
        }
    }
}

// -- DecodedSpectrum ---------------------------------------------------------

/// A single frame's peaks with calibrated m/z and inverse-mobility values.
///
/// Returned by `Reader.decode_spectrum()`. All three arrays are the same
/// length; index `i` describes one ion. Arrays are plain `numpy.ndarray`s
/// built by handing NumPy the already-allocated Rust buffer directly
/// (no per-element Python object / list conversion).
#[pyclass(module = "opentimstdf", name = "DecodedSpectrum")]
struct DecodedSpectrum {
    /// Calibrated m/z values (Da), `dtype=float64`.
    #[pyo3(get)]
    mz: Py<PyArray1<f64>>,
    /// Calibrated inverse ion mobility values (1/K0, V·s/cm²), `dtype=float64`.
    #[pyo3(get)]
    inv_mobility: Py<PyArray1<f64>>,
    /// Raw intensity counts, `dtype=uint32`.
    #[pyo3(get)]
    intensity: Py<PyArray1<u32>>,
}

#[pymethods]
impl DecodedSpectrum {
    fn __len__(&self, py: Python<'_>) -> usize {
        self.mz.bind(py).len()
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        format!("DecodedSpectrum({} peaks)", self.mz.bind(py).len())
    }
}

// -- Reader ------------------------------------------------------------------

/// Open a `.d/` (TDF) bundle and read its metadata and peak data.
///
/// Parameters
/// ----------
/// path : str
///     Path to the `.d/` directory.
#[pyclass(module = "opentimstdf", name = "Reader")]
struct Reader {
    inner: Mutex<RsReader>,
    bundle_dir: PathBuf,
}

impl Reader {
    /// Lock the underlying reader.
    fn locked_inner(&self) -> PyResult<std::sync::MutexGuard<'_, RsReader>> {
        self.inner
            .lock()
            .map_err(|_| PyRuntimeError::new_err("reader lock poisoned"))
    }
}

#[pymethods]
impl Reader {
    /// Canonical run metadata used by the Rust mzML writer.
    fn run_info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let guard = self.locked_inner()?;
        let name = self
            .bundle_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = ::opentimstdf::mzml::TdfSource::new(&guard, name).map_err(to_py_err)?;
        json_object(py, &source.run_metadata())
    }

    /// Canonical TIC, BPC, and scheduled PRM chromatograms.
    fn read_chromatograms(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let guard = self.locked_inner()?;
        let name = self
            .bundle_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut source = ::opentimstdf::mzml::TdfSource::new(&guard, name).map_err(to_py_err)?;
        source
            .iter_chromatograms()
            .map(|rec| json_object(py, &rec))
            .collect()
    }

    /// Stream full canonical spectrum records with a two-record buffer.
    fn iter_records(&self) -> PyResult<RecordIter> {
        let path = self.bundle_dir.clone();
        let source = ::opentimstdf::mzml::TdfSource::open(&path).map_err(to_py_err)?;
        let (sender, receiver) = sync_channel(2);
        std::thread::spawn(move || {
            let mut source = source;
            for record in source.iter_spectra() {
                if sender.send(Some(record)).is_err() {
                    return;
                }
            }
            let _ = sender.send(None);
        });
        Ok(RecordIter {
            receiver: Mutex::new(receiver),
            finished: false,
        })
    }

    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let r = RsReader::open(path).map_err(to_py_err)?;
        let bundle_dir = r.bundle_dir().to_path_buf();
        Ok(Self {
            inner: Mutex::new(r),
            bundle_dir,
        })
    }

    #[getter]
    fn bundle_dir(&self) -> String {
        self.bundle_dir.to_string_lossy().into_owned()
    }

    #[getter]
    fn compression_type(&self) -> PyResult<u32> {
        Ok(self.locked_inner()?.compression_type())
    }

    fn metadata(&self) -> PyResult<Metadata> {
        Ok(self.locked_inner()?.metadata().map_err(to_py_err)?.into())
    }

    fn calibration(&self) -> PyResult<Calibration> {
        let inner = self.locked_inner()?.calibration().map_err(to_py_err)?;
        Ok(Calibration { inner })
    }

    fn frame(&self, id: u32) -> PyResult<Frame> {
        Ok(self.locked_inner()?.frame(id).map_err(to_py_err)?.into())
    }

    fn frames(&self) -> PyResult<Vec<Frame>> {
        Ok(self
            .locked_inner()?
            .frames()
            .map_err(to_py_err)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn decode_peaks(&self, frame: &Frame) -> PyResult<Vec<Peak>> {
        let rs_frame = RsFrame {
            id: frame.id,
            time: frame.time,
            num_scans: frame.num_scans,
            num_peaks: frame.num_peaks,
            tims_id: frame.tims_id,
            polarity: frame.polarity_symbol.clone(),
            scan_mode: frame.scan_mode,
            msms_type: frame.msms_type,
            mz_calibration_id: frame.mz_calibration_id,
            accumulation_time: frame.accumulation_time,
            summed_intensities: frame.summed_intensities,
            max_intensity: frame.max_intensity,
        };
        Ok(self
            .locked_inner()?
            .decode_peaks(&rs_frame)
            .map_err(to_py_err)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Decode peaks for a frame and apply calibration in one step.
    ///
    /// Returns a `DecodedSpectrum` with parallel `mz`, `inv_mobility`, and
    /// `intensity` arrays. Equivalent to calling `decode_peaks` and then
    /// converting each `Peak` via the `Calibration` object, but in a single
    /// lock acquisition.
    fn decode_spectrum(&self, py: Python<'_>, frame: &Frame) -> PyResult<DecodedSpectrum> {
        let guard = self.locked_inner()?;
        let cal = guard.calibration().map_err(to_py_err)?;
        let rs_frame = RsFrame {
            id: frame.id,
            time: frame.time,
            num_scans: frame.num_scans,
            num_peaks: frame.num_peaks,
            tims_id: frame.tims_id,
            polarity: frame.polarity_symbol.clone(),
            scan_mode: frame.scan_mode,
            msms_type: frame.msms_type,
            mz_calibration_id: frame.mz_calibration_id,
            accumulation_time: frame.accumulation_time,
            summed_intensities: frame.summed_intensities,
            max_intensity: frame.max_intensity,
        };
        let peaks = guard.decode_peaks(&rs_frame).map_err(to_py_err)?;
        let n = peaks.len();
        let mut mz = Vec::with_capacity(n);
        let mut inv_mobility = Vec::with_capacity(n);
        let mut intensity = Vec::with_capacity(n);
        for p in peaks {
            mz.push(cal.tof_to_mz(p.tof));
            inv_mobility.push(cal.scan_to_inv_mobility(p.scan));
            intensity.push(p.intensity);
        }
        Ok(DecodedSpectrum {
            mz: mz.into_pyarray(py).unbind(),
            inv_mobility: inv_mobility.into_pyarray(py).unbind(),
            intensity: intensity.into_pyarray(py).unbind(),
        })
    }

    fn dia_windows_for_frame(&self, frame_id: u32) -> PyResult<Option<DiaFrameWindows>> {
        Ok(self
            .locked_inner()?
            .dia_windows_for_frame(frame_id)
            .map_err(to_py_err)?
            .map(Into::into))
    }

    fn pasef_msms_info_for_frame(&self, frame_id: u32) -> PyResult<Vec<PasefMsMsInfo>> {
        Ok(self
            .locked_inner()?
            .pasef_msms_info_for_frame(frame_id)
            .map_err(to_py_err)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn prm_msms_info_for_frame(&self, frame_id: u32) -> PyResult<Vec<PrmMsMsInfo>> {
        Ok(self
            .locked_inner()?
            .prm_msms_info_for_frame(frame_id)
            .map_err(to_py_err)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn prm_target(&self, target_id: u32) -> PyResult<Option<PrmTarget>> {
        Ok(self
            .locked_inner()?
            .prm_target(target_id)
            .map_err(to_py_err)?
            .map(Into::into))
    }

    fn precursor(&self, precursor_id: u32) -> PyResult<Option<Precursor>> {
        Ok(self
            .locked_inner()?
            .precursor(precursor_id)
            .map_err(to_py_err)?
            .map(Into::into))
    }

    fn __repr__(&self) -> String {
        format!("Reader(bundle_dir={:?})", self.bundle_dir.to_string_lossy())
    }
}

// -- Module ------------------------------------------------------------------

#[pymodule]
fn opentimstdf(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RecordIter>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<Reader>()?;
    m.add_class::<Calibration>()?;
    m.add_class::<Frame>()?;
    m.add_class::<Peak>()?;
    m.add_class::<DecodedSpectrum>()?;
    m.add_class::<Metadata>()?;
    m.add_class::<DiaWindow>()?;
    m.add_class::<DiaFrameWindows>()?;
    m.add_class::<PasefMsMsInfo>()?;
    m.add_class::<PrmMsMsInfo>()?;
    m.add_class::<PrmTarget>()?;
    m.add_class::<Precursor>()?;
    Ok(())
}
