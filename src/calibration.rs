/// Open-source linear-in-sqrt TOF↔m/z and linear scan↔1/K₀ calibration,
/// ported from `opentims++` (`tof2mz_converter.cpp`,
/// `scan2inv_ion_mobility_converter.cpp`). See SPEC §5 and §6.
///
/// These are NOT the Bruker polynomial models; they reproduce opentims'
/// open-source approximation, which is what downstream consumers get when
/// they don't link the proprietary Bruker library.
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
