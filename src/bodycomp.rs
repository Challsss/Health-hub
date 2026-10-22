//! Body composition estimates from weight + bioelectrical impedance.
//!
//! Formulas follow the community reverse-engineered Xiaomi Mi Scale algorithm
//! (as used by openScale and Home Assistant's `bodymiscale`). They are *estimates*
//! and will not exactly match the numbers shown in the Mi Home app.

use crate::model::{BodyComposition, ScaleMeasurement, Sex, UserProfile};

/// Valid impedance window (ohm). Values outside are treated as bad contact.
const IMPEDANCE_RANGE: std::ops::RangeInclusive<f32> = 100.0..=3000.0;

pub fn bmi(weight: f32, height_cm: f32) -> f32 {
    let h = height_cm / 100.0;
    weight / (h * h)
}

pub fn ideal_weight(height_cm: f32) -> f32 {
    let h = height_cm / 100.0;
    22.0 * h * h
}

struct Inputs {
    weight: f32,
    height: f32,
    age: f32,
    male: bool,
    /// Primary (50 kHz low-frequency) impedance in ohms.
    imp: f32,
    /// Secondary (250 kHz high-frequency) impedance in ohms, present on dual-frequency scales (S400).
    imp_hf: Option<f32>,
}

impl Inputs {
    fn is_dual_frequency(&self) -> bool {
        self.imp_hf.is_some()
    }

    /// Effective impedance combining 50 kHz (extracellular) and 250 kHz (intracellular)
    /// when dual-frequency S400 measurements are available.
    fn effective_impedance(&self) -> f32 {
        match self.imp_hf {
            Some(hf) => 0.70 * self.imp + 0.30 * hf,
            None => self.imp,
        }
    }

    fn lbm_coefficient(&self) -> f32 {
        let z = self.effective_impedance();
        let mut lbm = (self.height * 9.058 / 100.0) * (self.height / 100.0);
        lbm += self.weight * 0.32 + 12.226;
        lbm -= z * 0.0068;
        lbm -= self.age * 0.0542;
        if !self.male && self.is_dual_frequency() {
            // Sexual dimorphism adjustment for direct 2-compartment (Siri) S400 mode
            lbm *= 0.84;
        }
        lbm.min(self.weight * 0.98)
    }

    fn body_fat(&self) -> f32 {
        if self.is_dual_frequency() {
            // S400 dual-frequency mode: direct 2-compartment Siri model from calibrated LBM
            let lbm = self.lbm_coefficient();
            let bf = ((self.weight - lbm) / self.weight) * 100.0;
            let min_bf = if self.male { 5.0 } else { 10.0 };
            return bf.clamp(min_bf, 75.0);
        }

        // Legacy single-frequency Xiaomi / Zepp Life formula
        let lbm_sub = if !self.male && self.age <= 49.0 {
            9.25
        } else if !self.male {
            7.25
        } else {
            0.8
        };
        let mut coeff = 1.0;
        if self.male && self.weight < 61.0 {
            coeff = 0.98;
        } else if !self.male && self.weight > 60.0 {
            coeff = 0.96;
            if self.height > 160.0 {
                coeff *= 1.03;
            }
        } else if !self.male && self.weight < 50.0 {
            coeff = 1.02;
            if self.height > 160.0 {
                coeff *= 1.03;
            }
        }
        let bf = (1.0 - ((self.lbm_coefficient() - lbm_sub) * coeff) / self.weight) * 100.0;
        bf.clamp(3.0, 75.0)
    }

    fn bone_mass(&self) -> f32 {
        let base = if self.male { 0.18016894 } else { 0.245691014 };
        let mut bone = (base - self.lbm_coefficient() * 0.05158) * -1.0;
        if bone > 2.2 {
            bone += 0.1;
        } else {
            bone -= 0.1;
        }
        bone.clamp(0.5, 8.0)
    }

    fn water_pct(&self, body_fat: f32) -> f32 {
        if self.is_dual_frequency() {
            // Pace & Rathbun (1945) / S400 mode constant
            return ((100.0 - body_fat) * 0.73).clamp(35.0, 75.0);
        }
        let water = (100.0 - body_fat) * 0.7;
        let coeff = if water <= 50.0 { 1.02 } else { 0.98 };
        (water * coeff).clamp(35.0, 75.0)
    }

    fn visceral_fat(&self, body_fat: f32) -> f32 {
        let (w, h, a) = (self.weight, self.height, self.age);
        if self.is_dual_frequency() {
            // S400 dual-frequency BIA visceral fat estimate based on Fat Mass Index (FMI)
            // and age distribution (accounts for actual BIA fat mass vs muscular BMI).
            let h_m = h / 100.0;
            let fat_mass = w * (body_fat / 100.0);
            let fmi = fat_mass / (h_m * h_m);
            let v = if self.male {
                fmi + a * 0.05
            } else {
                fmi * 0.85 + a * 0.05
            };
            return v.clamp(1.0, 50.0);
        }

        // Standard Zepp Life formula (with fixed `+ 63.0` male threshold and `- 10.5` female offset)
        let v = if !self.male {
            if w > (13.0 - h * 0.5) * -1.0 {
                let subsub = (h * 1.45 + h * 0.1158 * h) - 120.0;
                let sub = w * 500.0 / subsub;
                (sub - 6.0) + a * 0.07
            } else {
                let sub = 0.691 + h * -0.0024;
                (w * sub - h * 0.027) + a * 0.07 - 10.5
            }
        } else if h < w * 1.6 + 63.0 {
            let sub = (h * 0.0826 * h) - (h * 0.4);
            ((w * 305.0) / (sub + 48.0)) - 2.9 + a * 0.15
        } else {
            let sub = 0.765 + h * -0.0015;
            (w * sub - h * 0.143) + a * 0.15 - 5.0
        };
        v.clamp(1.0, 50.0)
    }

    fn bmr(&self) -> f32 {
        let (w, h, a) = (self.weight, self.height, self.age);
        if self.is_dual_frequency() {
            // Katch-McArdle equation using BIA Lean Body Mass
            let lbm = self.lbm_coefficient();
            return (370.0 + 21.6 * lbm).clamp(500.0, 5000.0);
        }
        let v = if self.male {
            877.8 + w * 14.916 - h * 0.726 - a * 8.976
        } else {
            864.6 + w * 10.2036 - h * 0.39336 - a * 6.204
        };
        v.clamp(500.0, 5000.0)
    }

    fn metabolic_age(&self) -> f32 {
        let (w, h, a, i) = (self.weight, self.height, self.age, self.imp);
        if self.is_dual_frequency() {
            let bmr_actual = self.bmr();
            let bmr_expected = if self.male {
                88.362 + 13.397 * w + 4.799 * h - 5.677 * a
            } else {
                447.593 + 9.247 * w + 3.098 * h - 4.330 * a
            };
            if bmr_actual > 0.0 {
                return (a * (bmr_expected / bmr_actual)).clamp(15.0, 80.0);
            }
        }
        let v = if self.male {
            h * -0.7471 + w * 0.9161 + a * 0.4184 + i * 0.0517 + 54.2267
        } else {
            h * -1.1165 + w * 1.5784 + a * 0.4615 + i * 0.0415 + 83.2548
        };
        v.clamp(15.0, 80.0)
    }
}

/// Pick the impedance value to use (50 kHz preferred, as in the single-frequency formulas).
pub fn usable_impedance(m: &ScaleMeasurement) -> Option<f32> {
    [m.impedance_low, m.impedance_high]
        .into_iter()
        .flatten()
        .find(|v| IMPEDANCE_RANGE.contains(v))
}

/// Extract ordered (50 kHz low-freq, optional 250 kHz high-freq) valid impedances.
/// By BIA physics, 50 kHz resistance (`z_lf`) is always greater than 250 kHz (`z_hf`).
fn usable_impedances(m: &ScaleMeasurement) -> Option<(f32, Option<f32>)> {
    let z1 = m.impedance_low.filter(|v| IMPEDANCE_RANGE.contains(v));
    let z2 = m.impedance_high.filter(|v| IMPEDANCE_RANGE.contains(v));
    match (z1, z2) {
        (Some(a), Some(b)) => Some((a.max(b), Some(a.min(b)))),
        (Some(a), None) => Some((a, None)),
        (None, Some(b)) => Some((b, None)),
        (None, None) => None,
    }
}

/// Compute body composition for a measurement. Without valid impedance only
/// BMI / ideal weight (and any body fat reported by another app) are filled in.
pub fn compute(m: &ScaleMeasurement, profile: &UserProfile, age: u32) -> BodyComposition {
    let weight = m.weight_kg;
    let mut out = BodyComposition {
        weight_kg: weight,
        bmi: bmi(weight, profile.height_cm),
        ideal_weight_kg: ideal_weight(profile.height_cm),
        ..Default::default()
    };

    let Some((imp, imp_hf)) = usable_impedances(m) else {
        if let Some(bf) = m.reported_body_fat {
            out.body_fat_pct = Some(bf);
            out.fat_mass_kg = Some(weight * bf / 100.0);
            out.lean_mass_kg = Some(weight - weight * bf / 100.0);
        }
        return out;
    };

    let inp = Inputs {
        weight,
        height: profile.height_cm,
        age: age as f32,
        male: profile.sex == Sex::Male,
        imp,
        imp_hf,
    };
    let bf = inp.body_fat();
    let fat_mass = weight * bf / 100.0;
    let bone = inp.bone_mass();
    let lean = weight - fat_mass;
    let muscle = (lean - bone).max(0.0);
    let water = inp.water_pct(bf);
    let protein = if inp.is_dual_frequency() {
        ((inp.lbm_coefficient() * 0.195 / weight) * 100.0).clamp(5.0, 32.0)
    } else {
        ((muscle / weight) * 100.0 - water).clamp(5.0, 30.0)
    };

    out.body_fat_pct = Some(bf);
    out.fat_mass_kg = Some(fat_mass);
    out.lean_mass_kg = Some(lean);
    out.muscle_mass_kg = Some(muscle);
    out.bone_mass_kg = Some(bone);
    out.water_pct = Some(water);
    out.visceral_fat = Some(inp.visceral_fat(bf));
    out.protein_pct = Some(protein);
    out.bmr_kcal = Some(inp.bmr());
    out.metabolic_age = Some(inp.metabolic_age());
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rating {
    Low,
    Healthy,
    High,
    VeryHigh,
}

impl Rating {
    pub fn label(self) -> &'static str {
        match self {
            Rating::Low => "low",
            Rating::Healthy => "healthy",
            Rating::High => "high",
            Rating::VeryHigh => "very high",
        }
    }
}

/// Body fat ranges by age and sex (Gallagher et al. 2000, as used by ACE/most scales).
/// Returns (lower healthy bound, upper healthy bound, obese threshold).
pub fn body_fat_ranges(sex: Sex, age: u32) -> (f32, f32, f32) {
    match (sex, age) {
        (Sex::Male, 0..=39) => (8.0, 20.0, 25.0),
        (Sex::Male, 40..=59) => (11.0, 22.0, 28.0),
        (Sex::Male, _) => (13.0, 25.0, 30.0),
        (Sex::Female, 0..=39) => (21.0, 33.0, 39.0),
        (Sex::Female, 40..=59) => (23.0, 34.0, 40.0),
        (Sex::Female, _) => (24.0, 36.0, 42.0),
    }
}

pub fn rate_body_fat(bf: f32, sex: Sex, age: u32) -> Rating {
    let (lo, hi, obese) = body_fat_ranges(sex, age);
    if bf < lo {
        Rating::Low
    } else if bf < hi {
        Rating::Healthy
    } else if bf < obese {
        Rating::High
    } else {
        Rating::VeryHigh
    }
}

pub fn rate_bmi(bmi: f32) -> Rating {
    if bmi < 18.5 {
        Rating::Low
    } else if bmi < 25.0 {
        Rating::Healthy
    } else if bmi < 30.0 {
        Rating::High
    } else {
        Rating::VeryHigh
    }
}

/// Visceral fat rating (scale units, Tanita/Xiaomi convention).
pub fn rate_visceral(v: f32) -> Rating {
    if v < 10.0 {
        Rating::Healthy
    } else if v < 15.0 {
        Rating::High
    } else {
        Rating::VeryHigh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MeasurementSource;

    fn meas(weight: f32, imp: Option<f32>) -> ScaleMeasurement {
        ScaleMeasurement {
            time_ms: 0,
            weight_kg: weight,
            impedance_low: imp,
            impedance_high: None,
            heart_rate: None,
            profile_id: None,
            reported_body_fat: None,
            source: MeasurementSource::ScaleBle,
        }
    }

    fn male() -> UserProfile {
        UserProfile {
            height_cm: 180.0,
            birth_year: 1996,
            sex: Sex::Male,
        }
    }

    #[test]
    fn bmi_and_ideal() {
        assert!((bmi(80.0, 180.0) - 24.69).abs() < 0.01);
        assert!((ideal_weight(180.0) - 71.28).abs() < 0.01);
    }

    #[test]
    fn male_reference_values() {
        // Hand-computed from the formulas: lbm coeff = 62.148
        let c = compute(&meas(80.0, Some(500.0)), &male(), 30);
        let bf = c.body_fat_pct.unwrap();
        assert!((bf - 23.315).abs() < 0.05, "bf = {bf}");
        let bone = c.bone_mass_kg.unwrap();
        // (0.18016894 - 62.148*0.05158) * -1 = 3.0254, +0.1
        assert!((bone - 3.125).abs() < 0.01, "bone = {bone}");
        let lean = c.lean_mass_kg.unwrap();
        assert!((lean - (80.0 - 80.0 * bf / 100.0)).abs() < 1e-3);
        assert!((c.muscle_mass_kg.unwrap() - (lean - bone)).abs() < 1e-3);
        // BMR: 877.8 + 1193.28 - 130.68 - 269.28 = 1671.12
        assert!((c.bmr_kcal.unwrap() - 1671.12).abs() < 0.5);
        let water = c.water_pct.unwrap();
        assert!((40.0..65.0).contains(&water), "water = {water}");
        let v = c.visceral_fat.unwrap();
        assert!((1.0..30.0).contains(&v), "visceral = {v}");
    }

    #[test]
    fn higher_impedance_means_more_fat() {
        let p = male();
        let a = compute(&meas(80.0, Some(450.0)), &p, 30).body_fat_pct.unwrap();
        let b = compute(&meas(80.0, Some(650.0)), &p, 30).body_fat_pct.unwrap();
        assert!(b > a);
    }

    #[test]
    fn female_plausible() {
        let p = UserProfile {
            height_cm: 165.0,
            birth_year: 1990,
            sex: Sex::Female,
        };
        let c = compute(&meas(62.0, Some(600.0)), &p, 36);
        let bf = c.body_fat_pct.unwrap();
        assert!((15.0..45.0).contains(&bf), "bf = {bf}");
    }

    #[test]
    fn no_impedance_only_bmi() {
        let c = compute(&meas(80.0, None), &male(), 30);
        assert!(c.body_fat_pct.is_none());
        assert!(c.bmi > 0.0);
        // Bad contact value is rejected
        let c = compute(&meas(80.0, Some(5.0)), &male(), 30);
        assert!(c.body_fat_pct.is_none());
    }

    #[test]
    fn reported_body_fat_used_without_impedance() {
        let mut m = meas(80.0, None);
        m.reported_body_fat = Some(20.0);
        let c = compute(&m, &male(), 30);
        assert_eq!(c.body_fat_pct, Some(20.0));
        assert!((c.fat_mass_kg.unwrap() - 16.0).abs() < 1e-4);
    }

    #[test]
    fn s400_dual_frequency_calibration() {
        let p = UserProfile {
            height_cm: 178.0,
            birth_year: 2003,
            sex: Sex::Male,
        };
        let m = ScaleMeasurement {
            time_ms: 0,
            weight_kg: 81.5,
            impedance_low: Some(420.0),
            impedance_high: Some(373.0),
            heart_rate: None,
            profile_id: Some(1),
            reported_body_fat: None,
            source: MeasurementSource::ScaleBle,
        };
        let c = compute(&m, &p, 23);
        let bf = c.body_fat_pct.unwrap();
        assert!((bf - 22.7).abs() < 0.1, "s400 bf = {bf}");
        let vf = c.visceral_fat.unwrap();
        assert!((vf - 7.0).abs() < 0.2, "s400 visceral = {vf}");
        assert_eq!(rate_visceral(vf), Rating::Healthy);
    }

    #[test]
    fn ratings() {
        assert_eq!(rate_body_fat(15.0, Sex::Male, 30), Rating::Healthy);
        assert_eq!(rate_body_fat(26.0, Sex::Male, 30), Rating::VeryHigh);
        assert_eq!(rate_body_fat(18.0, Sex::Female, 30), Rating::Low);
        assert_eq!(rate_bmi(27.0), Rating::High);
        assert_eq!(rate_visceral(12.0), Rating::High);
    }
}
