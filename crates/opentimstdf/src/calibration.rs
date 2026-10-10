//! TOF index <-> m/z and scan index <-> 1/K0 conversion.
//!
//! See `docs/docs/format/04-calibration.md` for the derivation and the
//! validation figures.
//!
//! # m/z
//!
//! Two models are implemented. [`Reader::bundle_calibration`] picks one per
//! bundle and reports the choice in [`MzCalibrationStatus`].
//!
//! * **Tables** ([`TablesMzModel`], the default). Built from the
//!   `MzCalibration` row that each frame references (`Frames.MzCalibration`):
//!
//!   ```text
//!   t(tof)  = DigitizerDelay + DigitizerTimebase * tof            (ns)
//!   t(mz)   = C0 + (1e6 / sqrt(C1)) * sqrt(mz + C4) + C2 * mz     (ns)
//!   ```
//!
//!   `tof_to_mz` solves the second equation for `mz` in closed form (it is
//!   a quadratic in `sqrt(mz + C4)`). Only `ModelType = 1` with `C3 = 0` is
//!   supported; the meaning of `C3` has not been identified.
//! * **Range fallback**. Used only when the calibration tables are absent or
//!   unusable, with the reason recorded in
//!   [`MzCalibrationStatus::fallback_reason`]. Linear in `sqrt(mz)` between
//!   `GlobalMetadata.MzAcqRangeLower` and `MzAcqRangeUpper` over
//!   `DigitizerNumSamples` (the `opentims` open-source model,
//!   `tof2mz_converter.cpp`, BSD-2-Clause). It ignores the digitizer delay
//!   and can be off by hundreds of ppm.
//!
//! Neither model applies a temperature correction (`MzCalibration.T1`,
//! `T2`, `dC1`, `dC2` and the per-frame `Frames.T1`, `Frames.T2` are not
//! used).
//!
//! # 1/K0
//!
//! Linear in scan index between `GlobalMetadata.OneOverK0AcqRangeUpper`
//! (scan 0) and `OneOverK0AcqRangeLower` (scan `MAX(Frames.NumScans)`), the
//! `opentims` model (`scan2inv_ion_mobility_converter.cpp`). The
//! `TimsCalibration` table is not used.
//!
//! [`Reader::bundle_calibration`]: crate::Reader::bundle_calibration

use std::collections::BTreeMap;
use std::fmt;

/// Which TOF -> m/z model a [`Calibration`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MzCalibrationModel {
    /// [`TablesMzModel`], built from the bundle's `MzCalibration` table.
    Tables,
    /// Linear-in-sqrt(m/z) model built from the acquisition m/z range.
    RangeFallback,
}

impl MzCalibrationModel {
    /// Stable identifier: `"tables"` or `"range_fallback"`. This is the
    /// value written to the `opentimstdf.mz_calibration` run metadata key.
    pub const fn as_str(self) -> &'static str {
        match self {
            MzCalibrationModel::Tables => "tables",
            MzCalibrationModel::RangeFallback => "range_fallback",
        }
    }
}

impl fmt::Display for MzCalibrationModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which m/z model a bundle uses, and why the fallback was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MzCalibrationStatus {
    pub model: MzCalibrationModel,
    /// Why the calibration tables could not be used. `Some` exactly when
    /// `model` is [`MzCalibrationModel::RangeFallback`].
    pub fallback_reason: Option<String>,
}

/// TOF -> m/z model from one `MzCalibration` row (`ModelType = 1`).
///
/// ```text
/// t(tof) = digitizer_delay + digitizer_timebase * tof          (ns)
/// t(mz)  = c0 + (1e6 / sqrt(c1)) * sqrt(mz + c4) + c2 * mz     (ns)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TablesMzModel {
    /// `MzCalibration.DigitizerTimebase`: ns per TOF index.
    pub digitizer_timebase: f64,
    /// `MzCalibration.DigitizerDelay`: flight time of TOF index 0, in ns.
    pub digitizer_delay: f64,
    /// `MzCalibration.C0`: flight-time offset in ns.
    pub c0: f64,
    /// `MzCalibration.C1`: `mz = c1 * ((t - c0) * 1e-6)^2` when `c2 = c4 = 0`.
    pub c1: f64,
    /// `MzCalibration.C2`: flight-time term linear in m/z, in ns per Da.
    pub c2: f64,
    /// `MzCalibration.C4`: m/z offset inside the square root, in Da.
    pub c4: f64,
}

impl TablesMzModel {
    /// Flight-time slope `1e6 / sqrt(c1)`, in ns per sqrt(Da).
    fn k(&self) -> f64 {
        1e6 / self.c1.sqrt()
    }

    /// Flight time in ns of a (possibly fractional) TOF index.
    pub fn flight_time_ns(&self, tof: f64) -> f64 {
        self.digitizer_delay + self.digitizer_timebase * tof
    }

    /// m/z of a (possibly fractional) TOF index.
    pub fn tof_to_mz(&self, tof: f64) -> f64 {
        // With s = sqrt(mz + c4): c2*s^2 + k*s - u = 0, u = t - c0 + c2*c4.
        // The positive root, written so that c2 = 0 needs no special case.
        let k = self.k();
        let u = self.flight_time_ns(tof) - self.c0 + self.c2 * self.c4;
        let s = 2.0 * u / (k + (k * k + 4.0 * self.c2 * u).sqrt());
        s * s - self.c4
    }

    /// Fractional TOF index of an m/z (exact inverse of [`Self::tof_to_mz`]).
    pub fn mz_to_tof(&self, mz: f64) -> f64 {
        let t = self.c0 + self.k() * (mz + self.c4).sqrt() + self.c2 * mz;
        (t - self.digitizer_delay) / self.digitizer_timebase
    }

    /// Check that the model is finite and strictly increasing over TOF
    /// indices `0..=tof_max`. `tof_max` is `GlobalMetadata.DigitizerNumSamples`
    /// when the bundle records it; without it only index 0 is checked.
    pub fn validate(&self, tof_max: Option<u32>) -> std::result::Result<(), String> {
        let fields = [
            ("DigitizerTimebase", self.digitizer_timebase),
            ("DigitizerDelay", self.digitizer_delay),
            ("C0", self.c0),
            ("C1", self.c1),
            ("C2", self.c2),
            ("C4", self.c4),
        ];
        for (name, v) in fields {
            if !v.is_finite() {
                return Err(format!("{name} is not finite ({v})"));
            }
        }
        if self.digitizer_timebase <= 0.0 {
            return Err(format!(
                "DigitizerTimebase {} is not positive",
                self.digitizer_timebase
            ));
        }
        if self.c1 <= 0.0 {
            return Err(format!("C1 {} is not positive", self.c1));
        }
        // u is linear in tof, so checking both ends covers the range: the
        // root must stay real (discriminant > 0) and on the positive branch
        // (s > 0), which also makes m/z strictly increasing in tof.
        let k = self.k();
        let mut ends = vec![0.0];
        if let Some(n) = tof_max {
            ends.push(f64::from(n));
        }
        for tof in ends {
            let u = self.flight_time_ns(tof) - self.c0 + self.c2 * self.c4;
            let disc = k * k + 4.0 * self.c2 * u;
            if u <= 0.0 || disc <= 0.0 {
                return Err(format!("no positive m/z solution at TOF index {tof}"));
            }
            let mz = self.tof_to_mz(tof);
            if !(mz.is_finite() && mz > 0.0) {
                return Err(format!("m/z {mz} at TOF index {tof} is not positive"));
            }
        }
        Ok(())
    }
}

/// The TOF -> m/z half of a [`Calibration`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MzConversion {
    /// Model from the bundle's `MzCalibration` table.
    Tables(TablesMzModel),
    /// `sqrt(mz) = intercept + slope * tof`, from the acquisition m/z range.
    RangeFallback { intercept: f64, slope: f64 },
}

/// TOF <-> m/z and scan <-> 1/K0 conversion for one `MzCalibration` row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub mz: MzConversion,
    /// `1/K0 = im_intercept + im_slope * scan`
    pub im_intercept: f64,
    pub im_slope: f64,
}

impl Calibration {
    /// Which m/z model this calibration uses.
    pub fn mz_model(&self) -> MzCalibrationModel {
        match self.mz {
            MzConversion::Tables(_) => MzCalibrationModel::Tables,
            MzConversion::RangeFallback { .. } => MzCalibrationModel::RangeFallback,
        }
    }

    pub fn tof_to_mz(&self, tof: u32) -> f64 {
        self.tof_to_mz_f64(f64::from(tof))
    }

    /// [`Self::tof_to_mz`] for a fractional TOF index (for example a peak
    /// centroid).
    pub fn tof_to_mz_f64(&self, tof: f64) -> f64 {
        match self.mz {
            MzConversion::Tables(m) => m.tof_to_mz(tof),
            MzConversion::RangeFallback { intercept, slope } => {
                let v = intercept + slope * tof;
                v * v
            }
        }
    }

    /// Nearest TOF index of an m/z, clamped at 0.
    pub fn mz_to_tof(&self, mz: f64) -> u32 {
        let v = self.mz_to_tof_f64(mz);
        if v > 0.0 {
            (v + 0.5) as u32
        } else {
            0
        }
    }

    /// Fractional TOF index of an m/z, without rounding or clamping.
    pub fn mz_to_tof_f64(&self, mz: f64) -> f64 {
        match self.mz {
            MzConversion::Tables(m) => m.mz_to_tof(mz),
            MzConversion::RangeFallback { intercept, slope } => (mz.sqrt() - intercept) / slope,
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

/// Calibration for every `MzCalibration` row a bundle's frames reference.
///
/// Frames of different polarity reference different `MzCalibration` rows,
/// so converting a frame's peaks needs that frame's row:
/// use [`BundleCalibration::for_frame`].
#[derive(Debug, Clone)]
pub struct BundleCalibration {
    pub status: MzCalibrationStatus,
    primary: Calibration,
    by_row: BTreeMap<u32, Calibration>,
}

impl BundleCalibration {
    pub(crate) fn new(
        status: MzCalibrationStatus,
        primary: Calibration,
        by_row: BTreeMap<u32, Calibration>,
    ) -> Self {
        Self {
            status,
            primary,
            by_row,
        }
    }

    /// Calibration for the `MzCalibration` row referenced by the most frames
    /// (lowest row id on a tie). This is what [`crate::Reader::calibration`]
    /// returns. With the range fallback every row maps to it.
    pub fn primary(&self) -> &Calibration {
        &self.primary
    }

    /// Calibration for `Frames.MzCalibration = id`. Ids that no frame
    /// references map to [`Self::primary`].
    pub fn for_mz_calibration_id(&self, id: u32) -> &Calibration {
        self.by_row.get(&id).unwrap_or(&self.primary)
    }

    /// Calibration for a frame (by its `mz_calibration_id`).
    pub fn for_frame(&self, frame: &crate::Frame) -> &Calibration {
        self.for_mz_calibration_id(frame.mz_calibration_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row 1 of the PXD036417 `NQO1-F107C_coi-N2-P_200-0C_3996.d` bundle.
    fn linear_row() -> TablesMzModel {
        TablesMzModel {
            digitizer_timebase: 0.2,
            digitizer_delay: 44043.6,
            c0: 317.348_348_003_602_6,
            c1: 154_289.667_395_345_78,
            c2: 0.0,
            c4: 0.0,
        }
    }

    /// Row 2 of the same bundle: non-zero C2 and C4.
    fn curved_row() -> TablesMzModel {
        TablesMzModel {
            digitizer_timebase: 0.2,
            digitizer_delay: 44043.6,
            c0: 311.503_571,
            c1: 154_184.095_116,
            c2: -0.000_328,
            c4: 0.004_232,
        }
    }

    fn ppm(a: f64, b: f64) -> f64 {
        (a - b) / b * 1e6
    }

    #[test]
    fn model_type_names_are_stable() {
        assert_eq!(MzCalibrationModel::Tables.as_str(), "tables");
        assert_eq!(MzCalibrationModel::RangeFallback.as_str(), "range_fallback");
        assert_eq!(MzCalibrationModel::Tables.to_string(), "tables");
    }

    #[test]
    fn tables_linear_row_matches_closed_form() {
        // With c2 = c4 = 0: mz = c1 * ((t - c0) * 1e-6)^2.
        let m = linear_row();
        for tof in [0.0, 1000.0, 123_456.5, 291_332.0] {
            let t = m.flight_time_ns(tof);
            let want = m.c1 * ((t - m.c0) * 1e-6).powi(2);
            let got = m.tof_to_mz(tof);
            assert!(ppm(got, want).abs() < 1e-9, "tof {tof}: {got} vs {want}");
        }
    }

    #[test]
    fn tables_reproduces_bundle_reference_pairs() {
        // CalibrationInfo '+' of the same bundle: MeasuredTimesOfFlight (ns)
        // and MassesCorrectedCalibration (Da), which row 1 reproduces.
        let pairs = [
            (46_004.289_080_83, 322.048_291),
            (63_811.958_775_95, 622.028_908_32),
            (77_620.851_440_93, 922.009_068_5),
            (89_312.342_706_87, 1_221.990_986_57),
            (99_636.928_023_54, 1_521.971_740_61),
        ];
        let m = linear_row();
        for (t_ns, mz) in pairs {
            let tof = (t_ns - m.digitizer_delay) / m.digitizer_timebase;
            assert!(ppm(m.tof_to_mz(tof), mz).abs() < 1e-3, "{mz}");
        }
        // CalibrationInfo '-' pairs, reproduced by row 2 (C2, C4 non-zero).
        // The stored coefficients are rounded to 6 decimals, hence 0.02 ppm.
        let pairs = [
            (44_568.751_415, 301.998_123),
            (82_202.668_569, 1_033.987_998),
            (135_882.796_425, 2_833.872_863),
        ];
        let m = curved_row();
        for (t_ns, mz) in pairs {
            let tof = (t_ns - m.digitizer_delay) / m.digitizer_timebase;
            assert!(ppm(m.tof_to_mz(tof), mz).abs() < 0.02, "{mz}");
        }
    }

    #[test]
    fn tables_round_trip_and_monotonic() {
        for m in [linear_row(), curved_row()] {
            let mut prev = 0.0;
            for i in 0..=100 {
                let tof = f64::from(i) * 3_000.0;
                let mz = m.tof_to_mz(tof);
                assert!(mz > prev, "not increasing at {tof}");
                prev = mz;
                assert!((m.mz_to_tof(mz) - tof).abs() < 1e-6, "round trip at {tof}");
            }
        }
    }

    #[test]
    fn tables_c2_and_c4_terms() {
        // Synthetic: start from a flight time built with the forward
        // formula and check the inverse recovers the mass.
        let m = TablesMzModel {
            digitizer_timebase: 0.125,
            digitizer_delay: 30_000.0,
            c0: 300.0,
            c1: 155_000.0,
            c2: 0.002,
            c4: -0.07,
        };
        for mz in [150.0, 622.029, 1_700.0] {
            let t = m.c0 + 1e6 / m.c1.sqrt() * (mz + m.c4).sqrt() + m.c2 * mz;
            let tof = (t - m.digitizer_delay) / m.digitizer_timebase;
            assert!(ppm(m.tof_to_mz(tof), mz).abs() < 1e-9);
        }
    }

    #[test]
    fn validate_rejects_bad_rows() {
        assert!(linear_row().validate(Some(291_332)).is_ok());
        assert!(curved_row().validate(Some(291_332)).is_ok());

        let mut m = linear_row();
        m.c1 = 0.0;
        assert!(m.validate(None).unwrap_err().contains("C1"));

        let mut m = linear_row();
        m.digitizer_timebase = -0.2;
        assert!(m.validate(None).unwrap_err().contains("DigitizerTimebase"));

        let mut m = linear_row();
        m.c0 = f64::NAN;
        assert!(m.validate(None).unwrap_err().contains("C0"));

        // C0 beyond the flight time of index 0: no positive solution there.
        let mut m = linear_row();
        m.c0 = 50_000.0;
        assert!(m.validate(None).is_err());

        // A strongly negative C2 makes the root complex at the top of the range.
        let mut m = linear_row();
        m.c2 = -20.0;
        assert!(m.validate(None).is_ok());
        assert!(m.validate(Some(291_332)).is_err());
    }

    #[test]
    fn range_fallback_math() {
        let c = Calibration {
            mz: MzConversion::RangeFallback {
                intercept: 10.0,
                slope: 1e-4,
            },
            im_intercept: 1.6,
            im_slope: -0.001,
        };
        assert_eq!(c.mz_model(), MzCalibrationModel::RangeFallback);
        assert!((c.tof_to_mz(0) - 100.0).abs() < 1e-12);
        assert!((c.tof_to_mz(100_000) - 400.0).abs() < 1e-9);
        assert_eq!(c.mz_to_tof(400.0), 100_000);
        assert_eq!(c.mz_to_tof(1.0), 0);
        assert!((c.tof_to_mz_f64(50_000.5) - 225.001_500_002_5).abs() < 1e-6);
    }

    #[test]
    fn calibration_dispatches_to_tables() {
        let m = linear_row();
        let c = Calibration {
            mz: MzConversion::Tables(m),
            im_intercept: 1.6,
            im_slope: -0.001,
        };
        assert_eq!(c.mz_model(), MzCalibrationModel::Tables);
        assert_eq!(c.tof_to_mz(1234), m.tof_to_mz(1234.0));
        assert_eq!(c.mz_to_tof(c.tof_to_mz(1234)), 1234);
        assert_eq!(c.mz_to_tof(1.0), 0);
    }

    #[test]
    fn bundle_calibration_lookup() {
        let mk = |m: TablesMzModel| Calibration {
            mz: MzConversion::Tables(m),
            im_intercept: 1.6,
            im_slope: -0.001,
        };
        let a = mk(linear_row());
        let b = mk(curved_row());
        let bc = BundleCalibration::new(
            MzCalibrationStatus {
                model: MzCalibrationModel::Tables,
                fallback_reason: None,
            },
            a,
            BTreeMap::from([(1, a), (2, b)]),
        );
        assert_eq!(*bc.primary(), a);
        assert_eq!(*bc.for_mz_calibration_id(2), b);
        assert_eq!(*bc.for_mz_calibration_id(9), a);
    }
}
