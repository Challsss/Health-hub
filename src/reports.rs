//! Period reports: aggregation, trends and a rule-based recommendation engine.
//!
//! Guidelines used (general population, adults):
//! * Steps: ~7,500–10,000/day associated with lower all-cause mortality.
//! * Exercise: WHO – at least 150 min/week moderate activity.
//! * Sleep: AASM/NSF – 7–9 h for adults; consistent schedule.
//! * Resting HR / HRV: compared against the user's own baseline.
//! * Weight change: 0.25–1 % of body weight per week is considered sustainable.
//! * Body fat: Gallagher et al. (2000) ranges by age and sex.

use chrono::{Duration, NaiveDate};

use crate::bodycomp::{self, Rating};
use crate::model::{BodyComposition, DayMetrics, SleepSession};
use crate::store::{mean, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Week,
    Month,
    Quarter,
}

impl Period {
    pub fn days(self) -> i64 {
        match self {
            Period::Week => 7,
            Period::Month => 30,
            Period::Quarter => 90,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Period::Week => "Weekly",
            Period::Month => "Monthly",
            Period::Quarter => "Quarterly",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Alert,
    Warning,
    Info,
    Good,
}

impl Severity {
    pub fn icon(self) -> &'static str {
        match self {
            Severity::Good => "[OK]",
            Severity::Info => "[INFO]",
            Severity::Warning => "[WARN]",
            Severity::Alert => "[ALERT]",
        }
    }

    pub fn badge_label(self) -> &'static str {
        match self {
            Severity::Good => "GOOD",
            Severity::Info => "INFO",
            Severity::Warning => "WARNING",
            Severity::Alert => "ALERT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Activity,
    Sleep,
    Heart,
    Body,
    Combined,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Activity => "Activity",
            Category::Sleep => "Sleep",
            Category::Heart => "Heart",
            Category::Body => "Body",
            Category::Combined => "Insights",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub category: Category,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stat {
    pub label: String,
    pub value: String,
    /// Change vs. previous period, already formatted (e.g. "+5 %").
    pub change: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub stats: Vec<Stat>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub title: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub sections: Vec<Section>,
    pub recommendations: Vec<Recommendation>,
    pub notes: Vec<String>,
}

impl Report {
    /// Plain-text rendering (for copy/share).
    pub fn to_text(&self) -> String {
        let mut s = format!("{} ({} – {})\n\n", self.title, self.start, self.end);
        for sec in &self.sections {
            s.push_str(&format!("== {} ==\n", sec.title));
            for st in &sec.stats {
                s.push_str(&format!("  {}: {}", st.label, st.value));
                if let Some(c) = &st.change {
                    s.push_str(&format!(" ({c})"));
                }
                s.push('\n');
            }
            s.push('\n');
        }
        s.push_str("== Recommendations ==\n");
        for r in &self.recommendations {
            s.push_str(&format!(
                "{} [{}] {}\n    {}\n",
                r.severity.icon(),
                r.category.label(),
                r.title,
                r.detail
            ));
        }
        for n in &self.notes {
            s.push_str(&format!("\nNote: {n}"));
        }
        s.push_str(&format!("\n\n{DISCLAIMER}\n"));
        s
    }
}

pub const DISCLAIMER: &str = "These insights are generated from consumer-device estimates and general \
guidelines. They are not medical advice; consult a healthcare professional about health concerns.";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Least-squares slope of y over x.
pub fn slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let num: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let den: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    if den.abs() < 1e-12 {
        None
    } else {
        Some(num / den)
    }
}

fn std_dev(v: &[f64]) -> Option<f64> {
    let m = mean(v)?;
    if v.len() < 2 {
        return None;
    }
    Some((v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt())
}

fn pct_change(cur: f64, prev: f64) -> Option<f64> {
    if prev.abs() < 1e-9 {
        None
    } else {
        Some((cur - prev) / prev * 100.0)
    }
}

fn fmt_pct_change(cur: Option<f64>, prev: Option<f64>) -> Option<String> {
    let c = pct_change(cur?, prev?)?;
    Some(format!("{:+.0} % vs prev.", c))
}

fn fmt_abs_change(cur: Option<f64>, prev: Option<f64>, unit: &str, decimals: usize) -> Option<String> {
    let d = cur? - prev?;
    Some(format!("{:+.*} {unit} vs prev.", decimals, d))
}

fn fmt_opt(v: Option<f64>, unit: &str, decimals: usize) -> String {
    match v {
        Some(v) => format!("{:.*} {unit}", decimals, v).trim_end().to_string(),
        None => "–".to_string(),
    }
}

pub fn fmt_hours(min: f64) -> String {
    let m = min.round() as i64;
    format!("{}h {:02}m", m / 60, m % 60)
}

/// Minutes after midnight → "HH:MM".
pub fn fmt_clock(min: f64) -> String {
    let m = (min.round() as i64).rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

fn days_in(store: &Store, start: NaiveDate, end: NaiveDate) -> Vec<&DayMetrics> {
    store.days.range(start..=end).map(|(_, d)| d).collect()
}

/// Main (longest) sleep session per night within [start, end].
pub fn nights_in(store: &Store, start: NaiveDate, end: NaiveDate) -> Vec<&SleepSession> {
    let mut out = Vec::new();
    let mut d = start;
    while d <= end {
        if let Some(s) = store.sleep_for(d) {
            out.push(s);
        }
        d += Duration::days(1);
    }
    out
}

fn avg_of(days: &[&DayMetrics], f: impl Fn(&DayMetrics) -> Option<f64>) -> Option<f64> {
    let v: Vec<f64> = days.iter().filter_map(|d| f(d)).collect();
    mean(&v)
}

fn steps_of(d: &DayMetrics) -> Option<f64> {
    d.steps.filter(|s| *s > 0).map(|s| s as f64)
}

// ---------------------------------------------------------------------------
// Stats per domain
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Default, Debug, Clone, Copy)]
struct ActivityStats {
    days_with_data: usize,
    avg_steps: Option<f64>,
    days_10k: usize,
    days_7_5k: usize,
    days_under_5k: usize,
    avg_active_kcal: Option<f64>,
    avg_distance_km: Option<f64>,
    exercise_min_per_week: Option<f64>,
}

fn activity_stats(days: &[&DayMetrics], period_days: i64) -> ActivityStats {
    let steps: Vec<f64> = days.iter().filter_map(|d| steps_of(d)).collect();
    let ex: Vec<f64> = days.iter().filter_map(|d| d.exercise_min).collect();
    ActivityStats {
        days_with_data: steps.len(),
        avg_steps: mean(&steps),
        days_10k: steps.iter().filter(|s| **s >= 10_000.0).count(),
        days_7_5k: steps.iter().filter(|s| **s >= 7_500.0).count(),
        days_under_5k: steps.iter().filter(|s| **s < 5_000.0).count(),
        avg_active_kcal: avg_of(days, |d| d.active_kcal),
        avg_distance_km: avg_of(days, |d| d.distance_m.map(|m| m / 1000.0)),
        exercise_min_per_week: if ex.is_empty() {
            None
        } else {
            Some(ex.iter().sum::<f64>() * 7.0 / period_days as f64)
        },
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct SleepStats {
    nights: usize,
    avg_asleep_min: Option<f64>,
    nights_under_6h: usize,
    nights_7_to_9h: usize,
    bedtime_avg_min: Option<f64>,
    bedtime_sd_min: Option<f64>,
    deep_pct: Option<f64>,
    rem_pct: Option<f64>,
    awake_min: Option<f64>,
}

/// Bedtime minutes are measured relative to 18:00 to avoid the midnight wrap.
fn bedtime_offset(min: i32) -> f64 {
    ((min - 1080).rem_euclid(1440)) as f64
}

fn sleep_stats(nights: &[&SleepSession]) -> SleepStats {
    let asleep: Vec<f64> = nights.iter().map(|s| s.breakdown().asleep_min).collect();
    let bed: Vec<f64> = nights.iter().map(|s| bedtime_offset(s.bedtime_min)).collect();
    let staged: Vec<_> = nights
        .iter()
        .filter(|s| s.has_stage_detail())
        .map(|s| s.breakdown())
        .collect();
    let deep: Vec<f64> = staged
        .iter()
        .filter(|b| b.asleep_min > 0.0)
        .map(|b| b.deep_min / b.asleep_min * 100.0)
        .collect();
    let rem: Vec<f64> = staged
        .iter()
        .filter(|b| b.asleep_min > 0.0)
        .map(|b| b.rem_min / b.asleep_min * 100.0)
        .collect();
    let awake: Vec<f64> = staged.iter().map(|b| b.awake_min).collect();
    SleepStats {
        nights: nights.len(),
        avg_asleep_min: mean(&asleep),
        nights_under_6h: asleep.iter().filter(|m| **m < 360.0).count(),
        nights_7_to_9h: asleep.iter().filter(|m| (420.0..=540.0).contains(*m)).count(),
        bedtime_avg_min: mean(&bed).map(|m| m + 1080.0),
        bedtime_sd_min: std_dev(&bed),
        deep_pct: mean(&deep),
        rem_pct: mean(&rem),
        awake_min: mean(&awake),
    }
}

#[allow(dead_code)]
#[derive(Default, Debug, Clone, Copy)]
struct HeartStats {
    resting_hr: Option<f64>,
    resting_hr_slope_per_week: Option<f64>,
    hrv: Option<f64>,
    hrv_last7: Option<f64>,
    spo2_avg: Option<f64>,
    spo2_low_nights: usize,
    spo2_min: Option<f64>,
    resp_rate: Option<f64>,
}

fn heart_stats(days: &[&DayMetrics], start: NaiveDate, end: NaiveDate) -> HeartStats {
    let rhr_points: Vec<(f64, f64)> = days
        .iter()
        .filter_map(|d| d.resting_hr.map(|v| ((d.date - start).num_days() as f64, v)))
        .collect();
    let last7_start = end - Duration::days(6);
    let hrv_last7: Vec<f64> = days
        .iter()
        .filter(|d| d.date >= last7_start)
        .filter_map(|d| d.hrv_rmssd)
        .collect();
    HeartStats {
        resting_hr: avg_of(days, |d| d.resting_hr),
        resting_hr_slope_per_week: if rhr_points.len() >= 5 {
            slope(&rhr_points).map(|s| s * 7.0)
        } else {
            None
        },
        hrv: avg_of(days, |d| d.hrv_rmssd),
        hrv_last7: mean(&hrv_last7),
        spo2_avg: avg_of(days, |d| d.spo2_avg),
        spo2_low_nights: days
            .iter()
            .filter(|d| d.spo2_min.is_some_and(|v| v < 90.0))
            .count(),
        spo2_min: days.iter().filter_map(|d| d.spo2_min).reduce(f64::min),
        resp_rate: avg_of(days, |d| d.resp_rate),
    }
}

#[derive(Default, Debug, Clone)]
struct BodyStats {
    count: usize,
    first: Option<(i64, BodyComposition)>,
    last: Option<(i64, BodyComposition)>,
    /// kg/week, regression over all weigh-ins in the period.
    weight_slope_week: Option<f64>,
}

fn body_stats(store: &Store, start: NaiveDate, end: NaiveDate) -> BodyStats {
    let profile = store.profile();
    let start_ms = start.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
    let end_ms = (end + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    let ms: Vec<_> = store
        .scale
        .iter()
        .filter(|m| m.time_ms >= start_ms && m.time_ms < end_ms)
        .collect();
    let comp = |m: &crate::model::ScaleMeasurement| {
        let date = chrono::DateTime::from_timestamp_millis(m.time_ms)
            .map(|d| d.date_naive())
            .unwrap_or(end);
        (m.time_ms, bodycomp::compute(m, &profile, profile.age_on(date)))
    };
    let pts: Vec<(f64, f64)> = ms
        .iter()
        .map(|m| ((m.time_ms - start_ms) as f64 / 86_400_000.0, m.weight_kg as f64))
        .collect();
    BodyStats {
        count: ms.len(),
        first: ms.first().map(|m| comp(m)),
        last: ms.last().map(|m| comp(m)),
        weight_slope_week: if pts.len() >= 3 {
            slope(&pts).map(|s| s * 7.0)
        } else {
            None
        },
    }
}

// ---------------------------------------------------------------------------
// Report builder
// ---------------------------------------------------------------------------

fn rec(category: Category, severity: Severity, title: impl Into<String>, detail: impl Into<String>) -> Recommendation {
    Recommendation {
        category,
        severity,
        title: title.into(),
        detail: detail.into(),
    }
}

/// Build a report covering the `period` complete days before `today`.
pub fn build_report(store: &Store, period: Period, today: NaiveDate) -> Report {
    let n = period.days();
    let end = today - Duration::days(1);
    let start = today - Duration::days(n);
    let prev_end = start - Duration::days(1);
    let prev_start = start - Duration::days(n);

    let days = days_in(store, start, end);
    let prev_days = days_in(store, prev_start, prev_end);
    let nights = nights_in(store, start, end);
    let prev_nights = nights_in(store, prev_start, prev_end);

    let act = activity_stats(&days, n);
    let pact = activity_stats(&prev_days, n);
    let slp = sleep_stats(&nights);
    let pslp = sleep_stats(&prev_nights);
    let hrt = heart_stats(&days, start, end);
    let phrt = heart_stats(&prev_days, prev_start, prev_end);
    // HRV baseline: 30 days before the last 7 days of the period.
    let base_days = days_in(store, end - Duration::days(36), end - Duration::days(7));
    let hrv_baseline = avg_of(&base_days, |d| d.hrv_rmssd);
    let body = body_stats(store, start, end);

    let profile = store.profile();
    let age = profile.age_on(today);

    let mut sections = Vec::new();
    let mut recs = Vec::new();
    let mut notes = Vec::new();

    // ---- Activity ----
    sections.push(Section {
        title: "Activity".into(),
        stats: vec![
            Stat {
                label: "Average steps".into(),
                value: fmt_opt(act.avg_steps, "", 0),
                change: fmt_pct_change(act.avg_steps, pact.avg_steps),
            },
            Stat {
                label: "Days >= 10,000 steps".into(),
                value: format!("{} / {}", act.days_10k, act.days_with_data),
                change: None,
            },
            Stat {
                label: "Average distance".into(),
                value: fmt_opt(act.avg_distance_km, "km", 1),
                change: fmt_pct_change(act.avg_distance_km, pact.avg_distance_km),
            },
            Stat {
                label: "Active calories / day".into(),
                value: fmt_opt(act.avg_active_kcal, "kcal", 0),
                change: fmt_pct_change(act.avg_active_kcal, pact.avg_active_kcal),
            },
            Stat {
                label: "Exercise / week".into(),
                value: fmt_opt(act.exercise_min_per_week, "min", 0),
                change: fmt_pct_change(act.exercise_min_per_week, pact.exercise_min_per_week),
            },
        ],
    });

    if let Some(avg) = act.avg_steps {
        if avg >= 10_000.0 {
            recs.push(rec(Category::Activity, Severity::Good, "Excellent step count",
                format!("You averaged {avg:.0} steps/day and reached 10k on {} of {} days. Keep it up.", act.days_10k, act.days_with_data)));
        } else if avg >= 7_500.0 {
            recs.push(rec(Category::Activity, Severity::Good, "Solid daily movement",
                format!("{avg:.0} steps/day is in the health-benefit range. To reach 10k, add one 15–20 min walk (~2,000 steps) on your lower days.")));
        } else if avg >= 5_000.0 {
            recs.push(rec(Category::Activity, Severity::Warning, "Below the 7,500-step target",
                format!("You averaged {avg:.0} steps/day. Try a 10-minute walk after each main meal – that alone adds roughly 3,000 steps and helps blood sugar control.")));
        } else {
            recs.push(rec(Category::Activity, Severity::Alert, "Mostly sedentary period",
                format!("Average of {avg:.0} steps/day ({} days under 5,000). Start small: set a goal of +1,000 steps/day each week and stand up/move every hour.", act.days_under_5k)));
        }
        if let Some(c) = pact.avg_steps.and_then(|p| pct_change(avg, p)) {
            if c <= -15.0 {
                recs.push(rec(Category::Activity, Severity::Warning, "Activity dropped",
                    format!("Steps are down {:.0} % compared with the previous period. If this wasn't planned rest, schedule walks into your calendar.", -c)));
            } else if c >= 10.0 {
                recs.push(rec(Category::Activity, Severity::Good, "Activity is trending up",
                    format!("Steps are up {c:.0} % versus the previous period.")));
            }
        }
    } else {
        notes.push("No step data in this period – sync Health Connect.".into());
    }
    if let Some(ex) = act.exercise_min_per_week {
        if ex >= 150.0 {
            recs.push(rec(Category::Activity, Severity::Good, "Meeting the WHO exercise guideline",
                format!("~{ex:.0} min of exercise per week (target >= 150 min). Add 2 strength sessions/week if you aren't already.")));
        } else {
            recs.push(rec(Category::Activity, Severity::Info, "More structured exercise",
                format!("~{ex:.0} min/week of logged exercise. WHO recommends >= 150 min of moderate activity (brisk walking counts) plus 2 strength sessions per week.")));
        }
    }

    // ---- Sleep ----
    sections.push(Section {
        title: "Sleep".into(),
        stats: vec![
            Stat {
                label: "Average sleep".into(),
                value: slp.avg_asleep_min.map(fmt_hours).unwrap_or("–".into()),
                change: fmt_abs_change(slp.avg_asleep_min, pslp.avg_asleep_min, "min", 0),
            },
            Stat {
                label: "Nights 7–9 h".into(),
                value: format!("{} / {}", slp.nights_7_to_9h, slp.nights),
                change: None,
            },
            Stat {
                label: "Average bedtime".into(),
                value: slp.bedtime_avg_min.map(fmt_clock).unwrap_or("–".into()),
                change: None,
            },
            Stat {
                label: "Bedtime variability".into(),
                value: fmt_opt(slp.bedtime_sd_min, "min", 0),
                change: None,
            },
            Stat {
                label: "Deep sleep".into(),
                value: fmt_opt(slp.deep_pct, "%", 0),
                change: None,
            },
            Stat {
                label: "REM sleep".into(),
                value: fmt_opt(slp.rem_pct, "%", 0),
                change: None,
            },
            Stat {
                label: "Awake time / night".into(),
                value: fmt_opt(slp.awake_min, "min", 0),
                change: None,
            },
        ],
    });

    if let Some(avg) = slp.avg_asleep_min {
        let h = avg / 60.0;
        if h < 6.0 {
            recs.push(rec(Category::Sleep, Severity::Alert, "Significant sleep debt",
                format!("You slept {} on average. Chronic sleep under 6 h affects mood, appetite, weight and recovery. Move bedtime 30 min earlier this week and keep screens out of the last hour.", fmt_hours(avg))));
        } else if h < 7.0 {
            recs.push(rec(Category::Sleep, Severity::Warning, "Slightly short on sleep",
                format!("Average {} – adults need 7–9 h. Going to bed 20–30 minutes earlier would close most of the gap.", fmt_hours(avg))));
        } else if h <= 9.0 {
            recs.push(rec(Category::Sleep, Severity::Good, "Healthy sleep duration",
                format!("Average {} – right in the 7–9 h range.", fmt_hours(avg))));
        } else {
            recs.push(rec(Category::Sleep, Severity::Info, "Long sleep duration",
                format!("Average {}. Regularly needing more than 9 h can indicate poor sleep quality or illness – worth keeping an eye on.", fmt_hours(avg))));
        }
        if slp.nights >= 3 && slp.nights_under_6h as f64 / slp.nights as f64 >= 0.4 {
            recs.push(rec(Category::Sleep, Severity::Warning, "Frequent short nights",
                format!("{} of {} nights were under 6 hours.", slp.nights_under_6h, slp.nights)));
        }
    } else {
        notes.push("No sleep data in this period.".into());
    }
    if let Some(sd) = slp.bedtime_sd_min {
        if sd > 60.0 {
            recs.push(rec(Category::Sleep, Severity::Warning, "Irregular sleep schedule",
                format!("Your bedtime varies by ±{sd:.0} min. A consistent bed/wake time (even on weekends, within ~30 min) improves sleep quality and daytime energy.")));
        } else if sd <= 30.0 && slp.nights >= 5 {
            recs.push(rec(Category::Sleep, Severity::Good, "Consistent schedule",
                format!("Bedtime varies only ±{sd:.0} min – great for your circadian rhythm.")));
        }
    }
    if let Some(deep) = slp.deep_pct {
        if deep < 10.0 {
            recs.push(rec(Category::Sleep, Severity::Info, "Low deep sleep",
                format!("Deep sleep averaged {deep:.0} % (typical 13–23 %). Avoid alcohol and heavy meals within 3 h of bed, keep the bedroom cool (~18 °C), and exercise earlier in the day.")));
        }
    }
    if let Some(rem) = slp.rem_pct {
        if rem < 15.0 {
            recs.push(rec(Category::Sleep, Severity::Info, "Low REM sleep",
                format!("REM averaged {rem:.0} % (typical 20–25 %). REM is concentrated in the last hours of sleep, so cutting nights short reduces it most.")));
        }
    }

    // ---- Heart ----
    sections.push(Section {
        title: "Heart & recovery".into(),
        stats: vec![
            Stat {
                label: "Resting heart rate".into(),
                value: fmt_opt(hrt.resting_hr, "bpm", 0),
                change: fmt_abs_change(hrt.resting_hr, phrt.resting_hr, "bpm", 1),
            },
            Stat {
                label: "HRV (RMSSD)".into(),
                value: fmt_opt(hrt.hrv, "ms", 0),
                change: fmt_pct_change(hrt.hrv, phrt.hrv),
            },
            Stat {
                label: "HRV last 7 days vs baseline".into(),
                value: match (hrt.hrv_last7, hrv_baseline) {
                    (Some(a), Some(b)) => format!("{a:.0} vs {b:.0} ms"),
                    _ => "–".into(),
                },
                change: None,
            },
            Stat {
                label: "SpO2 average / lowest".into(),
                value: format!("{} / {}", fmt_opt(hrt.spo2_avg, "%", 1), fmt_opt(hrt.spo2_min, "%", 0)),
                change: None,
            },
            Stat {
                label: "Breathing rate".into(),
                value: fmt_opt(hrt.resp_rate, "br/min", 1),
                change: None,
            },
        ],
    });

    if let (Some(cur), Some(prev)) = (hrt.resting_hr, phrt.resting_hr) {
        let d = cur - prev;
        if d >= 3.0 {
            recs.push(rec(Category::Heart, Severity::Warning, "Resting heart rate is up",
                format!("Resting HR rose {d:.1} bpm vs the previous period ({prev:.0} to {cur:.0}). Common causes: poor sleep, stress, alcohol, illness or overtraining. Prioritise recovery for a few days.")));
        } else if d <= -2.0 {
            recs.push(rec(Category::Heart, Severity::Good, "Resting heart rate improving",
                format!("Resting HR dropped {:.1} bpm ({prev:.0} to {cur:.0}) – a typical sign of improving cardiovascular fitness.", -d)));
        }
    }
    if let Some(r) = hrt.resting_hr {
        if r > 80.0 {
            recs.push(rec(Category::Heart, Severity::Warning, "High resting heart rate",
                format!("An average resting HR of {r:.0} bpm is above the typical range. Regular aerobic exercise usually lowers it; if it stays high, discuss it with a doctor.")));
        }
    }
    if let (Some(last7), Some(base)) = (hrt.hrv_last7, hrv_baseline) {
        if let Some(c) = pct_change(last7, base) {
            if c <= -15.0 {
                recs.push(rec(Category::Heart, Severity::Warning, "Recovery is low (HRV down)",
                    format!("Your HRV over the last 7 days is {:.0} % below your 30-day baseline. Favour easy workouts, sleep, hydration and less alcohol until it recovers.", -c)));
            } else if c >= 10.0 {
                recs.push(rec(Category::Heart, Severity::Good, "Recovery is strong (HRV up)",
                    format!("HRV is {c:.0} % above your baseline – a good time for harder training sessions.")));
            }
        }
    }
    if hrt.spo2_low_nights >= 2 {
        recs.push(rec(Category::Heart, Severity::Alert, "Repeated low blood-oxygen readings",
            format!("SpO2 dipped below 90 % on {} nights. Wrist sensors can be noisy, but if you snore, wake up tired or have morning headaches, ask a doctor about sleep-apnea screening.", hrt.spo2_low_nights)));
    } else if hrt.spo2_avg.is_some_and(|v| v < 95.0) {
        recs.push(rec(Category::Heart, Severity::Info, "SpO2 slightly low",
            "Your average overnight SpO2 is below 95 %. Make sure the band fits snugly during sleep for accurate readings."));
    }

    // ---- Body ----
    let last_comp = body.last.map(|l| l.1);
    let first_comp = body.first.map(|f| f.1);
    let wchange = match (first_comp, last_comp) {
        (Some(a), Some(b)) if body.count >= 2 => Some((b.weight_kg - a.weight_kg) as f64),
        _ => None,
    };
    let fopt = |f: fn(&BodyComposition) -> Option<f32>| last_comp.as_ref().and_then(f).map(|v| v as f64);
    let fchg = |f: fn(&BodyComposition) -> Option<f32>| match (first_comp.as_ref().and_then(f), last_comp.as_ref().and_then(f)) {
        (Some(a), Some(b)) if body.count >= 2 => Some((b - a) as f64),
        _ => None,
    };
    let chg_fmt = |c: Option<f64>, unit: &str, dec: usize| c.map(|v| format!("{:+.*} {unit} in period", dec, v));

    sections.push(Section {
        title: "Body composition".into(),
        stats: vec![
            Stat {
                label: "Weigh-ins".into(),
                value: body.count.to_string(),
                change: None,
            },
            Stat {
                label: "Weight (latest)".into(),
                value: fmt_opt(last_comp.map(|c| c.weight_kg as f64), "kg", 1),
                change: chg_fmt(wchange, "kg", 1),
            },
            Stat {
                label: "BMI".into(),
                value: fmt_opt(last_comp.map(|c| c.bmi as f64), "", 1),
                change: None,
            },
            Stat {
                label: "Body fat".into(),
                value: fmt_opt(fopt(|c| c.body_fat_pct), "%", 1),
                change: chg_fmt(fchg(|c| c.body_fat_pct), "pts", 1),
            },
            Stat {
                label: "Muscle mass".into(),
                value: fmt_opt(fopt(|c| c.muscle_mass_kg), "kg", 1),
                change: chg_fmt(fchg(|c| c.muscle_mass_kg), "kg", 1),
            },
            Stat {
                label: "Body water".into(),
                value: fmt_opt(fopt(|c| c.water_pct), "%", 1),
                change: None,
            },
            Stat {
                label: "Visceral fat".into(),
                value: fmt_opt(fopt(|c| c.visceral_fat), "", 0),
                change: None,
            },
            Stat {
                label: "BMR".into(),
                value: fmt_opt(fopt(|c| c.bmr_kcal), "kcal", 0),
                change: None,
            },
        ],
    });

    if store.profile.is_none() {
        recs.push(rec(Category::Body, Severity::Info, "Complete your profile",
            "Enter height, birth year and sex in Settings – they are required for body-composition estimates."));
    }
    if body.count == 0 {
        recs.push(rec(Category::Body, Severity::Info, "No weigh-ins this period",
            "Weigh yourself 2–3 times per week, barefoot, in the morning after the bathroom and before eating, for comparable readings."));
    }
    if let Some(c) = last_comp {
        let w0 = first_comp.map(|f| f.weight_kg as f64).unwrap_or(c.weight_kg as f64);
        let weekly = body.weight_slope_week.or_else(|| {
            let weeks = match (body.first, body.last) {
                (Some(a), Some(b)) => (b.0 - a.0) as f64 / (7.0 * 86_400_000.0),
                _ => 0.0,
            };
            if weeks >= 1.0 { wchange.map(|w| w / weeks) } else { None }
        });
        if let Some(wk) = weekly {
            let pct = wk / w0 * 100.0;
            if pct < -1.0 {
                recs.push(rec(Category::Body, Severity::Warning, "Losing weight quickly",
                    format!("You're losing ~{:.2} kg/week ({:.1} %/week). Faster than 1 %/week raises the risk of muscle loss – consider a smaller calorie deficit.", -wk, -pct)));
            } else if pct <= -0.25 {
                recs.push(rec(Category::Body, Severity::Good, "Sustainable weight loss",
                    format!("~{:.2} kg/week ({:.1} %/week) is within the recommended 0.25–1 % range.", -wk, -pct)));
            } else if pct >= 0.5 {
                recs.push(rec(Category::Body, Severity::Warning, "Weight is rising",
                    format!("Gaining ~{wk:.2} kg/week. If you aren't intentionally building muscle, review portion sizes and evening snacking.")));
            } else if pct.abs() < 0.25 {
                recs.push(rec(Category::Body, Severity::Info, "Weight is stable",
                    format!("Weight changed by {wk:+.2} kg/week – essentially maintenance.")));
            }
        }
        if let Some(bf) = c.body_fat_pct {
            let (lo, hi, _) = bodycomp::body_fat_ranges(profile.sex, age);
            match bodycomp::rate_body_fat(bf, profile.sex, age) {
                Rating::Healthy => recs.push(rec(Category::Body, Severity::Good, "Body fat in healthy range",
                    format!("{bf:.1} % (healthy range for your age/sex: {lo:.0}–{hi:.0} %)."))),
                Rating::Low => recs.push(rec(Category::Body, Severity::Warning, "Body fat below healthy range",
                    format!("{bf:.1} % is under {lo:.0} %. Very low body fat can affect hormones and immunity; make sure you're eating enough."))),
                Rating::High => recs.push(rec(Category::Body, Severity::Warning, "Body fat above healthy range",
                    format!("{bf:.1} % (healthy: {lo:.0}–{hi:.0} %). A modest calorie deficit (~300–500 kcal/day), 2–3 strength sessions per week and high protein intake work best together."))),
                Rating::VeryHigh => recs.push(rec(Category::Body, Severity::Alert, "Body fat well above healthy range",
                    format!("{bf:.1} % is in the obese range for your age/sex (healthy: {lo:.0}–{hi:.0} %). Reducing it substantially lowers cardiometabolic risk – consider support from a doctor or dietitian."))),
            }
        } else if bodycomp::rate_bmi(c.bmi) != Rating::Healthy {
            recs.push(rec(Category::Body, Severity::Info, format!("BMI is {}", bodycomp::rate_bmi(c.bmi).label()),
                format!("BMI {:.1}. Step on the scale barefoot so it can measure impedance – body fat is a better indicator than BMI.", c.bmi)));
        }
        if let Some(v) = c.visceral_fat {
            match bodycomp::rate_visceral(v) {
                Rating::High => recs.push(rec(Category::Body, Severity::Warning, "Elevated visceral fat",
                    format!("Visceral fat level {v:.0} (healthy < 10). It responds well to aerobic exercise, less alcohol and fewer refined carbs."))),
                Rating::VeryHigh => recs.push(rec(Category::Body, Severity::Alert, "High visceral fat",
                    format!("Visceral fat level {v:.0} (healthy < 10) is linked to metabolic risk. Prioritise daily activity and discuss with a doctor."))),
                _ => {}
            }
        }
        if let Some(bmr) = c.bmr_kcal {
            if let Some(active) = act.avg_active_kcal {
                recs.push(rec(Category::Body, Severity::Info, "Your estimated energy needs",
                    format!("BMR ~{bmr:.0} kcal + ~{active:.0} kcal activity = ~{:.0} kcal/day to maintain weight.", bmr + active as f32)));
            }
        }
    }

    // ---- Cross-metric insights ----
    let muscle_chg = fchg(|c| c.muscle_mass_kg);
    if let (Some(w), Some(m)) = (wchange, muscle_chg) {
        if w < -0.5 && m < 0.0 && (-m) > 0.3 * (-w) {
            recs.push(rec(Category::Combined, Severity::Warning, "Weight loss includes muscle",
                format!("Of {:.1} kg lost, ~{:.1} kg appears to be muscle. Aim for ~1.6 g protein per kg body weight (~{:.0} g/day) and 2–3 strength sessions per week.",
                    -w, -m, 1.6 * last_comp.map(|c| c.weight_kg).unwrap_or(70.0))));
        }
        if w > 0.5 && fchg(|c| c.body_fat_pct).is_some_and(|f| f > 0.5) {
            recs.push(rec(Category::Combined, Severity::Info, "Recent gain is mostly fat",
                "Both weight and body-fat percentage went up. Increasing daily steps is the easiest lever to reverse this."));
        }
    }
    // HRV / resting HR after short vs long nights
    let mut hrv_short = Vec::new();
    let mut hrv_long = Vec::new();
    let mut rhr_short = Vec::new();
    let mut rhr_long = Vec::new();
    for s in &nights {
        let asleep = s.breakdown().asleep_min;
        if let Some(d) = store.days.get(&s.date) {
            let (hv, rv) = if asleep < 360.0 {
                (&mut hrv_short, &mut rhr_short)
            } else if asleep >= 420.0 {
                (&mut hrv_long, &mut rhr_long)
            } else {
                continue;
            };
            if let Some(h) = d.hrv_rmssd {
                hv.push(h);
            }
            if let Some(r) = d.resting_hr {
                rv.push(r);
            }
        }
    }
    if hrv_short.len() >= 2 && hrv_long.len() >= 2 {
        let (a, b) = (mean(&hrv_short).unwrap(), mean(&hrv_long).unwrap());
        if let Some(c) = pct_change(a, b) {
            if c <= -10.0 {
                recs.push(rec(Category::Combined, Severity::Info, "Short sleep lowers your recovery",
                    format!("On nights under 6 h your HRV averaged {a:.0} ms vs {b:.0} ms after 7 h+ nights ({:.0} % lower). Your body clearly benefits from full nights.", -c)));
            }
        }
    }
    if rhr_short.len() >= 2 && rhr_long.len() >= 2 {
        let (a, b) = (mean(&rhr_short).unwrap(), mean(&rhr_long).unwrap());
        if a - b >= 2.0 {
            recs.push(rec(Category::Combined, Severity::Info, "Short sleep raises your resting HR",
                format!("Resting HR averages {a:.0} bpm after short nights vs {b:.0} bpm after 7 h+ nights.")));
        }
    }
    // Activity → sleep: active days vs low days, sleep the following night
    let mut sleep_after_active = Vec::new();
    let mut sleep_after_low = Vec::new();
    for d in &days {
        if let (Some(steps), Some(s)) = (steps_of(d), store.sleep_for(d.date + Duration::days(1))) {
            let m = s.breakdown().asleep_min;
            if steps >= 10_000.0 {
                sleep_after_active.push(m);
            } else if steps < 6_000.0 {
                sleep_after_low.push(m);
            }
        }
    }
    if sleep_after_active.len() >= 2 && sleep_after_low.len() >= 2 {
        let (a, b) = (mean(&sleep_after_active).unwrap(), mean(&sleep_after_low).unwrap());
        if a - b >= 20.0 {
            recs.push(rec(Category::Combined, Severity::Good, "Active days help you sleep",
                format!("After 10k+ step days you slept {} vs {} after low-activity days.", fmt_hours(a), fmt_hours(b))));
        }
    }

    if act.days_with_data > 0 && (act.days_with_data as i64) < n / 2 {
        notes.push(format!(
            "Only {} of {} days have activity data – results may be less reliable.",
            act.days_with_data, n
        ));
    }

    recs.sort_by_key(|r| r.severity);

    Report {
        title: format!("{} report", period.label()),
        start,
        end,
        sections,
        recommendations: recs,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MeasurementSource, ScaleMeasurement, Sex, SleepStage, StageKind, UserProfile};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn ms(d: NaiveDate, h: u32) -> i64 {
        d.and_hms_opt(h, 0, 0).unwrap().and_utc().timestamp_millis()
    }

    fn store_with(steps: u64, sleep_h: f64, rhr: f64, today: NaiveDate) -> Store {
        let mut s = Store::in_memory();
        s.profile = Some(UserProfile { height_cm: 180.0, birth_year: 1990, sex: Sex::Male });
        for i in 1..=14 {
            let d = today - Duration::days(i);
            let mut m = DayMetrics::new(d);
            m.steps = Some(steps);
            m.resting_hr = Some(rhr);
            m.hrv_rmssd = Some(45.0);
            s.days.insert(d, m);
            let end = ms(d, 7);
            let start = end - (sleep_h * 3_600_000.0) as i64;
            s.sleep.push(SleepSession {
                start_ms: start,
                end_ms: end,
                date: d,
                bedtime_min: 23 * 60,
                stages: vec![SleepStage { start_ms: start, end_ms: end, kind: StageKind::Light }],
                source: "test".into(),
            });
        }
        s
    }

    fn has(r: &Report, title_part: &str) -> bool {
        r.recommendations.iter().any(|x| x.title.contains(title_part))
    }

    #[test]
    fn slope_basic() {
        let s = slope(&[(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)]).unwrap();
        assert!((s - 2.0).abs() < 1e-9);
        assert!(slope(&[(1.0, 1.0)]).is_none());
    }

    #[test]
    fn clock_and_hours() {
        assert_eq!(fmt_clock(23.0 * 60.0 + 30.0), "23:30");
        assert_eq!(fmt_clock(1440.0 + 15.0), "00:15");
        assert_eq!(fmt_hours(455.0), "7h 35m");
    }

    #[test]
    fn active_well_slept_user() {
        let today = date(2026, 10, 22);
        let s = store_with(11_000, 7.5, 55.0, today);
        let r = build_report(&s, Period::Week, today);
        assert!(has(&r, "Excellent step count"));
        assert!(has(&r, "Healthy sleep duration"));
        assert!(has(&r, "Consistent schedule"));
        assert!(!r.recommendations.iter().any(|x| x.severity == Severity::Alert));
    }

    #[test]
    fn sedentary_short_sleeper() {
        let today = date(2026, 10, 22);
        let s = store_with(3_000, 5.0, 70.0, today);
        let r = build_report(&s, Period::Week, today);
        assert!(has(&r, "Mostly sedentary"));
        assert!(has(&r, "Significant sleep debt"));
        assert!(has(&r, "Frequent short nights"));
        // alerts sorted first
        assert_eq!(r.recommendations[0].severity, Severity::Alert);
    }

    #[test]
    fn resting_hr_increase_detected() {
        let today = date(2026, 10, 22);
        let mut s = store_with(9_000, 7.5, 55.0, today);
        for i in 1..=7 {
            s.days.get_mut(&(today - Duration::days(i))).unwrap().resting_hr = Some(60.0);
        }
        let r = build_report(&s, Period::Week, today);
        assert!(has(&r, "Resting heart rate is up"));
    }

    #[test]
    fn fast_weight_loss_and_muscle_loss() {
        let today = date(2026, 10, 22);
        let mut s = store_with(9_000, 7.5, 55.0, today);
        // 30-day period: 90 kg → 85 kg with constant impedance (muscle falls with weight)
        for (i, w) in [(28, 90.0), (21, 88.5), (14, 87.0), (7, 86.0), (1, 85.0)] {
            s.add_measurement(ScaleMeasurement {
                time_ms: ms(today - Duration::days(i), 7),
                weight_kg: w,
                impedance_low: Some(480.0),
                impedance_high: None,
                heart_rate: None,
                profile_id: Some(1),
                reported_body_fat: None,
                source: MeasurementSource::ScaleBle,
            });
        }
        let r = build_report(&s, Period::Month, today);
        assert!(has(&r, "Losing weight quickly"), "{:#?}", r.recommendations);
        assert!(has(&r, "Weight loss includes muscle"), "{:#?}", r.recommendations);
        let text = r.to_text();
        assert!(text.contains("Body composition"));
        assert!(text.contains("not medical advice"));
    }

    #[test]
    fn empty_store_does_not_panic() {
        let s = Store::in_memory();
        let r = build_report(&s, Period::Quarter, date(2026, 10, 22));
        assert!(has(&r, "Complete your profile"));
        assert!(!r.notes.is_empty());
    }
}
