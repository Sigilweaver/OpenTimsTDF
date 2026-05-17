/// Descriptor for a single frame (mirrors the `Frames` SQLite row we need).
#[derive(Debug, Clone)]
pub struct Frame {
    pub id: u32,
    /// Retention time (seconds).
    pub time: f64,
    pub num_scans: u32,
    pub num_peaks: u32,
    pub tims_id: u64,
    /// Scan mode: 0 = MS1, 8 = PASEF DDA, 9 = diaPASEF.
    pub scan_mode: u32,
    /// MS/MS type: 0 = MS1, 8 = PASEF MS2, 9 = diaPASEF MS2.
    pub msms_type: u32,
    /// FK into `MzCalibration` table (1 = positive polarity, 2 = negative).
    pub mz_calibration_id: u32,
    /// Ion accumulation time in milliseconds (None if the column is absent).
    pub accumulation_time: Option<f64>,
    /// Raw sum of decoded intensities divided by (AccumulationTime/100).
    /// See SPEC §2.2 for the normalization formula.
    pub summed_intensities: Option<u64>,
}

/// One decoded peak.
#[derive(Debug, Clone, Copy)]
pub struct Peak {
    pub scan: u32,
    pub tof: u32,
    pub intensity: u32,
}

/// Bundle-level metadata from `GlobalMetadata`.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub schema_version_major: u32,
    pub schema_version_minor: u32,
    pub instrument_name: String,
    pub acquisition_software: String,
    pub acquisition_software_version: String,
    pub compression_type: u32,
}

/// One isolation window within a diaPASEF window group (SPEC §8.1).
#[derive(Debug, Clone)]
pub struct DiaWindow {
    pub window_group: u32,
    pub scan_num_begin: u32,
    pub scan_num_end: u32,
    pub isolation_mz: f64,
    pub isolation_width: f64,
    pub collision_energy: f64,
}

/// All diaPASEF isolation windows for a single MS2 frame.
#[derive(Debug, Clone)]
pub struct DiaFrameWindows {
    pub frame_id: u32,
    pub window_group: u32,
    pub windows: Vec<DiaWindow>,
}

/// One PASEF DDA MS2 scan range within a frame (SPEC §8.2).
#[derive(Debug, Clone)]
pub struct PasefMsMsInfo {
    pub frame_id: u32,
    pub scan_num_begin: u32,
    pub scan_num_end: u32,
    pub isolation_mz: f64,
    pub isolation_width: f64,
    pub collision_energy: f64,
    pub precursor_id: u32,
}

/// One prm-PASEF MS2 scan range within a frame (SPEC §8.3).
#[derive(Debug, Clone)]
pub struct PrmMsMsInfo {
    pub frame_id: u32,
    pub scan_num_begin: u32,
    pub scan_num_end: u32,
    pub isolation_mz: f64,
    pub isolation_width: f64,
    pub collision_energy: f64,
    /// FK into `PrmTargets.Id`.
    pub target_id: u32,
}

/// One scheduled PRM target from the `PrmTargets` table (SPEC §8.3).
#[derive(Debug, Clone)]
pub struct PrmTarget {
    pub id: u32,
    /// User-supplied target label (e.g. peptide name or compound ID).
    pub external_id: String,
    /// Scheduled retention time in minutes.
    pub time: f64,
    pub one_over_k0: f64,
    pub monoisotopic_mz: f64,
    pub charge: u32,
    /// Free-text annotation.
    pub description: String,
}

/// One PASEF DDA precursor (SPEC §8.2).
#[derive(Debug, Clone)]
pub struct Precursor {
    pub id: u32,
    pub largest_peak_mz: f64,
    pub average_mz: f64,
    pub monoisotopic_mz: Option<f64>,
    pub charge: Option<u32>,
    pub scan_number: f64,
    pub intensity: f64,
    pub parent_frame_id: u32,
}
