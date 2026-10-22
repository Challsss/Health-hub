//! Local persistence (a single JSON file) and merging of synced data.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::model::{
    DayMetrics, MeasurementSource, ScaleMeasurement, SleepSession, SleepStage, StageKind,
    UserProfile,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// 32 hex char MiBeacon bindkey of the S400.
    pub bindkey: String,
    /// Only accept S400 measurements for this scale user slot (None = any).
    pub scale_profile_id: Option<u8>,
    /// Restrict to this scale MAC (learned automatically on first successful decrypt).
    pub scale_mac: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub profile: Option<UserProfile>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub days: BTreeMap<NaiveDate, DayMetrics>,
    #[serde(default)]
    pub sleep: Vec<SleepSession>,
    #[serde(default)]
    pub scale: Vec<ScaleMeasurement>,
    #[serde(default)]
    pub last_sync_ms: Option<i64>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Store {
    pub fn load(path: &Path) -> Self {
        let mut s: Store = std::fs::read_to_string(path)
            .ok()
            .and_then(|txt| match serde_json::from_str(&txt) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::error!("Could not parse store {path:?}: {e}");
                    // keep a copy of the unreadable file instead of overwriting it
                    let _ = std::fs::copy(path, path.with_extension("corrupt.json"));
                    None
                }
            })
            .unwrap_or_default();
        s.path = Some(path.to_path_buf());
        s
    }

    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        let txt = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, txt).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn profile(&self) -> UserProfile {
        self.profile.clone().unwrap_or_default()
    }

    /// Insert a scale measurement, merging near-duplicates (same weigh-in seen twice).
    pub fn add_measurement(&mut self, m: ScaleMeasurement) -> bool {
        if let Some(existing) = self.scale.iter_mut().find(|e| {
            (e.time_ms - m.time_ms).abs() < 120_000 && (e.weight_kg - m.weight_kg).abs() < 0.15
        }) {
            // keep the richer record
            let richness = |x: &ScaleMeasurement| {
                x.impedance_low.is_some() as u8
                    + x.impedance_high.is_some() as u8
                    + x.heart_rate.is_some() as u8
                    + (x.source == MeasurementSource::ScaleBle) as u8
            };
            if richness(&m) > richness(existing) {
                *existing = m;
                return true;
            }
            return false;
        }
        self.scale.push(m);
        self.scale.sort_by_key(|x| x.time_ms);
        true
    }

    pub fn remove_measurement(&mut self, time_ms: i64) {
        self.scale.retain(|m| m.time_ms != time_ms);
    }

    /// Merge a Health Connect sync result. Returns a short human readable summary.
    pub fn merge_health(&mut self, h: HealthImport) -> String {
        let mut days_updated = 0usize;
        for d in h.days {
            let Some(date) = parse_date(&d.date) else { continue };
            let e = self.days.entry(date).or_insert_with(|| DayMetrics::new(date));
            e.steps = d.steps.map(|v| v.max(0) as u64).or(e.steps);
            e.distance_m = d.distance_m.or(e.distance_m);
            e.active_kcal = d.active_kcal.or(e.active_kcal);
            e.total_kcal = d.total_kcal.or(e.total_kcal);
            e.exercise_min = d.exercise_min.or(e.exercise_min);
            e.avg_hr = d.avg_hr.or(e.avg_hr);
            e.min_hr = d.min_hr.or(e.min_hr);
            e.max_hr = d.max_hr.or(e.max_hr);
            e.resting_hr = d.resting_hr.or(e.resting_hr);
            days_updated += 1;
        }

        // Daily averages of point samples.
        let mut hrv: BTreeMap<NaiveDate, Vec<f64>> = BTreeMap::new();
        for s in h.hrv {
            if let Some(d) = parse_date(&s.date) {
                hrv.entry(d).or_default().push(s.value);
            }
        }
        for (d, v) in hrv {
            let e = self.days.entry(d).or_insert_with(|| DayMetrics::new(d));
            e.hrv_rmssd = mean(&v);
        }
        let mut spo2: BTreeMap<NaiveDate, Vec<f64>> = BTreeMap::new();
        for s in h.spo2 {
            if let Some(d) = parse_date(&s.date) {
                spo2.entry(d).or_default().push(s.value);
            }
        }
        for (d, v) in spo2 {
            let e = self.days.entry(d).or_insert_with(|| DayMetrics::new(d));
            e.spo2_avg = mean(&v);
            e.spo2_min = v.iter().cloned().reduce(f64::min);
        }
        let mut resp: BTreeMap<NaiveDate, Vec<f64>> = BTreeMap::new();
        for s in h.resp {
            if let Some(d) = parse_date(&s.date) {
                resp.entry(d).or_default().push(s.value);
            }
        }
        for (d, v) in resp {
            let e = self.days.entry(d).or_insert_with(|| DayMetrics::new(d));
            e.resp_rate = mean(&v);
        }

        let mut sleep_added = 0usize;
        for s in h.sleep {
            let Some(date) = parse_date(&s.date) else { continue };
            let session = SleepSession {
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                date,
                bedtime_min: s.bedtime_min,
                stages: s
                    .stages
                    .into_iter()
                    .map(|st| SleepStage {
                        start_ms: st.start_ms,
                        end_ms: st.end_ms,
                        kind: StageKind::from_hc(st.stage),
                    })
                    .collect(),
                source: s.source,
            };
            if let Some(existing) = self.sleep.iter_mut().find(|e| e.start_ms == session.start_ms) {
                *existing = session;
            } else {
                self.sleep.push(session);
                sleep_added += 1;
            }
        }
        self.sleep.sort_by_key(|s| s.start_ms);

        let mut weights_added = 0usize;
        for w in h.weight {
            let bf = h
                .body_fat
                .iter()
                .find(|b| (b.time_ms - w.time_ms).abs() < 120_000)
                .map(|b| b.value as f32);
            let m = ScaleMeasurement {
                time_ms: w.time_ms,
                weight_kg: w.kg as f32,
                impedance_low: None,
                impedance_high: None,
                heart_rate: None,
                profile_id: None,
                reported_body_fat: bf,
                source: MeasurementSource::HealthConnect,
            };
            if self.add_measurement(m) {
                weights_added += 1;
            }
        }

        let mut summary = format!(
            "Synced {days_updated} days, {sleep_added} new sleep sessions, {weights_added} weights"
        );
        if !h.errors.is_empty() {
            summary.push_str(&format!(" ({} warnings)", h.errors.len()));
        }
        summary
    }

    pub fn latest_measurement(&self) -> Option<&ScaleMeasurement> {
        self.scale.last()
    }

    pub fn sleep_for(&self, date: NaiveDate) -> Option<&SleepSession> {
        // the longest session ending on that date (ignore naps when a night exists)
        self.sleep
            .iter()
            .filter(|s| s.date == date)
            .max_by_key(|s| s.end_ms - s.start_ms)
    }
}

pub fn mean(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        Some(v.iter().sum::<f64>() / v.len() as f64)
    }
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

// ---------- Health Connect import schema (produced by HealthBridge.kt) ----------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HealthImport {
    pub days: Vec<HcDay>,
    pub sleep: Vec<HcSleep>,
    pub hrv: Vec<HcSample>,
    pub spo2: Vec<HcSample>,
    pub resp: Vec<HcSample>,
    pub weight: Vec<HcWeight>,
    pub body_fat: Vec<HcTimed>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcDay {
    pub date: String,
    pub steps: Option<i64>,
    pub distance_m: Option<f64>,
    pub active_kcal: Option<f64>,
    pub total_kcal: Option<f64>,
    pub exercise_min: Option<f64>,
    pub avg_hr: Option<f64>,
    pub min_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub resting_hr: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcSleep {
    pub start_ms: i64,
    pub end_ms: i64,
    pub date: String,
    pub bedtime_min: i32,
    pub stages: Vec<HcStage>,
    pub source: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcStage {
    pub start_ms: i64,
    pub end_ms: i64,
    pub stage: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcSample {
    pub time_ms: i64,
    pub date: String,
    pub value: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcWeight {
    pub time_ms: i64,
    pub kg: f64,
    pub source: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HcTimed {
    pub time_ms: i64,
    pub value: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "days":[{"date":"2026-10-20","steps":8123,"distanceM":6100.5,"activeKcal":420,"restingHr":58}],
      "sleep":[{"startMs":1000,"endMs":28801000,"date":"2026-10-20","bedtimeMin":1380,
                "stages":[{"startMs":1000,"endMs":3601000,"stage":4},{"startMs":3601000,"endMs":7201000,"stage":5}],
                "source":"com.google.android.apps.fitness"}],
      "hrv":[{"timeMs":5,"date":"2026-10-20","value":40},{"timeMs":6,"date":"2026-10-20","value":50}],
      "spo2":[{"timeMs":5,"date":"2026-10-20","value":96},{"timeMs":6,"date":"2026-10-20","value":93}],
      "weight":[{"timeMs":100000,"kg":80.2,"source":"x"}],
      "bodyFat":[{"timeMs":100500,"value":21.5}],
      "errors":[]
    }"#;

    #[test]
    fn merge_sample_json() {
        let mut s = Store::in_memory();
        let h: HealthImport = serde_json::from_str(SAMPLE).unwrap();
        s.merge_health(h);
        let d = s.days[&NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()].clone();
        assert_eq!(d.steps, Some(8123));
        assert_eq!(d.resting_hr, Some(58.0));
        assert_eq!(d.hrv_rmssd, Some(45.0));
        assert_eq!(d.spo2_min, Some(93.0));
        assert_eq!(s.sleep.len(), 1);
        let b = s.sleep[0].breakdown();
        assert!((b.deep_min - 60.0).abs() < 1e-6);
        assert!((b.asleep_min - 120.0).abs() < 1e-6);
        assert_eq!(s.scale.len(), 1);
        assert_eq!(s.scale[0].reported_body_fat, Some(21.5));

        // re-sync is idempotent
        let h: HealthImport = serde_json::from_str(SAMPLE).unwrap();
        s.merge_health(h);
        assert_eq!(s.sleep.len(), 1);
        assert_eq!(s.scale.len(), 1);
    }

    #[test]
    fn ble_measurement_replaces_hc_duplicate() {
        let mut s = Store::in_memory();
        let base = ScaleMeasurement {
            time_ms: 0,
            weight_kg: 80.0,
            impedance_low: None,
            impedance_high: None,
            heart_rate: None,
            profile_id: None,
            reported_body_fat: None,
            source: MeasurementSource::HealthConnect,
        };
        assert!(s.add_measurement(base.clone()));
        let mut ble = base.clone();
        ble.time_ms = 30_000;
        ble.impedance_low = Some(500.0);
        ble.source = MeasurementSource::ScaleBle;
        assert!(s.add_measurement(ble));
        assert_eq!(s.scale.len(), 1);
        assert_eq!(s.scale[0].source, MeasurementSource::ScaleBle);
        // poorer duplicate does not replace
        assert!(!s.add_measurement(base));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hh_test_{}", std::process::id()));
        let path = dir.join("store.json");
        let mut s = Store::load(&path);
        s.settings.bindkey = "abc".into();
        s.profile = Some(UserProfile::default());
        s.save().unwrap();
        let s2 = Store::load(&path);
        assert_eq!(s2.settings.bindkey, "abc");
        assert!(s2.profile.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
