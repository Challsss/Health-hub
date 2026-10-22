//! Core data types shared by every part of the app.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Male,
    Female,
}

/// Personal data needed for body-composition formulas and age/sex specific ranges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub height_cm: f32,
    pub birth_year: i32,
    pub sex: Sex,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            height_cm: 175.0,
            birth_year: 1990,
            sex: Sex::Male,
        }
    }
}

impl UserProfile {
    pub fn age_on(&self, date: NaiveDate) -> u32 {
        use chrono::Datelike;
        (date.year() - self.birth_year).clamp(1, 120) as u32
    }
}

/// One calendar day of wearable metrics (merged from Health Connect).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DayMetrics {
    pub date: NaiveDate,
    pub steps: Option<u64>,
    pub distance_m: Option<f64>,
    pub active_kcal: Option<f64>,
    pub total_kcal: Option<f64>,
    pub exercise_min: Option<f64>,
    pub avg_hr: Option<f64>,
    pub min_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub resting_hr: Option<f64>,
    /// Heart-rate variability (RMSSD, milliseconds), daily average.
    pub hrv_rmssd: Option<f64>,
    pub spo2_avg: Option<f64>,
    pub spo2_min: Option<f64>,
    pub resp_rate: Option<f64>,
}

impl DayMetrics {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            ..Default::default()
        }
    }
}

/// Health Connect sleep stage codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageKind {
    Unknown,
    Awake,
    Sleeping,
    OutOfBed,
    Light,
    Deep,
    Rem,
    AwakeInBed,
}

impl StageKind {
    pub fn from_hc(code: i32) -> Self {
        match code {
            1 => Self::Awake,
            2 => Self::Sleeping,
            3 => Self::OutOfBed,
            4 => Self::Light,
            5 => Self::Deep,
            6 => Self::Rem,
            7 => Self::AwakeInBed,
            _ => Self::Unknown,
        }
    }

    pub fn is_asleep(self) -> bool {
        matches!(self, Self::Sleeping | Self::Light | Self::Deep | Self::Rem)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SleepStage {
    pub start_ms: i64,
    pub end_ms: i64,
    pub kind: StageKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SleepSession {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Local calendar date on which the session ended ("the night of").
    pub date: NaiveDate,
    /// Local bedtime expressed as minutes after midnight (can be negative-wrapped by consumers).
    pub bedtime_min: i32,
    pub stages: Vec<SleepStage>,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SleepBreakdown {
    pub in_bed_min: f64,
    pub asleep_min: f64,
    pub awake_min: f64,
    pub light_min: f64,
    pub deep_min: f64,
    pub rem_min: f64,
}

impl SleepSession {
    pub fn breakdown(&self) -> SleepBreakdown {
        let in_bed_min = (self.end_ms - self.start_ms) as f64 / 60_000.0;
        if self.stages.is_empty() {
            return SleepBreakdown {
                in_bed_min,
                asleep_min: in_bed_min,
                ..Default::default()
            };
        }
        let mut b = SleepBreakdown {
            in_bed_min,
            ..Default::default()
        };
        for s in &self.stages {
            let m = (s.end_ms - s.start_ms).max(0) as f64 / 60_000.0;
            match s.kind {
                StageKind::Light => b.light_min += m,
                StageKind::Deep => b.deep_min += m,
                StageKind::Rem => b.rem_min += m,
                StageKind::Sleeping => {}
                StageKind::Awake | StageKind::AwakeInBed | StageKind::OutOfBed => b.awake_min += m,
                StageKind::Unknown => {}
            }
            if s.kind.is_asleep() {
                b.asleep_min += m;
            }
        }
        b
    }

    pub fn has_stage_detail(&self) -> bool {
        self.stages
            .iter()
            .any(|s| matches!(s.kind, StageKind::Light | StageKind::Deep | StageKind::Rem))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeasurementSource {
    /// Decoded directly from the Xiaomi S400 Bluetooth broadcast.
    ScaleBle,
    /// Typed in by the user.
    Manual,
    /// Imported from Health Connect (e.g. Google Health weight log).
    HealthConnect,
}

/// A single weigh-in. Body composition is derived on demand from the profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScaleMeasurement {
    pub time_ms: i64,
    pub weight_kg: f32,
    /// ~50 kHz impedance (ohm).
    pub impedance_low: Option<f32>,
    /// ~250 kHz impedance (ohm).
    pub impedance_high: Option<f32>,
    pub heart_rate: Option<u8>,
    pub profile_id: Option<u8>,
    /// Body fat reported by another app (Health Connect import).
    pub reported_body_fat: Option<f32>,
    pub source: MeasurementSource,
}

/// Derived body-composition estimate for one measurement.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyComposition {
    pub weight_kg: f32,
    pub bmi: f32,
    pub body_fat_pct: Option<f32>,
    pub fat_mass_kg: Option<f32>,
    pub lean_mass_kg: Option<f32>,
    pub muscle_mass_kg: Option<f32>,
    pub water_pct: Option<f32>,
    pub bone_mass_kg: Option<f32>,
    pub visceral_fat: Option<f32>,
    pub protein_pct: Option<f32>,
    pub bmr_kcal: Option<f32>,
    pub metabolic_age: Option<f32>,
    pub ideal_weight_kg: f32,
}
