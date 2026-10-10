/// Linear-in-sqrt TOF<->m/z and linear scan<->1/K0 calibration, following
/// the open-source `opentims++` model (`tof2mz_converter.cpp`,
/// `scan2inv_ion_mobility_converter.cpp`). See
/// `docs/docs/format/04-calibration.md`.
///
/// This is NOT the proprietary polynomial model carried in
/// `MzCalibration` / `TimsCalibration`. `Reader::calibration()` builds the
/// m/z side from the acquisition m/z range in `GlobalMetadata` (the
/// "boundary variant" in the format docs), which can be off by up to
/// ~15000 ppm on some bundles. The < 2 ppm figure in the docs applies to the
/// regressed variant fitted from `CalibrationInfo`, which is not
/// implemented.
#[derive(Debug, Clone, Copy)]
pub struct Calibration {
    /// `sqrt(mz) = mz_intercept + mz_slope * tof`
    pub mz_intercept: f64,
    pub mz_slope: f64,
    /// `1/K₀ = im_intercept + im_slope * scan`
    pub im_intercept: f64,
    pub im_slope: f64,
}

impl Calibration {
    pub fn tof_to_mz(&self, tof: u32) -> f64 {
        let v = self.mz_intercept + self.mz_slope * f64::from(tof);
        v * v
    }

    pub fn mz_to_tof(&self, mz: f64) -> u32 {
        let v = (mz.sqrt() - self.mz_intercept) / self.mz_slope;
        if v > 0.0 {
            (v + 0.5) as u32
        } else {
            0
        }
    }

    pub fn scan_to_inv_mobility(&self, scan: u32) -> f64 {
        self.im_intercept + self.im_slope * f64::from(scan)
    }

    pub fn inv_mobility_to_scan(&self, inv_mobility: f64) -> u32 {
        let v = (inv_mobility - self.im_intercept) / self.im_slope;
        if v > 0.0 {
            (v + 0.5) as u32
        } else {
            0
        }
    }
}
