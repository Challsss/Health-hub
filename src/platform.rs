//! Platform bridge:
//! - On Android: talks to `com.example.androidapp.MainActivity` via JNI (Health Connect, BLE scan,
//!   system clipboard, and safe-area window insets).
//! - On Desktop: provides realistic demo data and simulated S400 BLE packets for testing.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use chrono::{Duration, Local, NaiveDate};

use crate::model::{
    DayMetrics, MeasurementSource, ScaleMeasurement, SleepSession, SleepStage, StageKind,
};
use crate::store::{HealthImport, Store};

#[derive(Debug, Clone)]
pub struct BleRawAdvertisement {
    pub mac: String,
    pub rssi: i32,
    pub data_hex: String,
    pub time_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardTarget {
    Bindkey,
    ScaleMac,
}

#[derive(Debug)]
pub enum PlatformEvent {
    Status(String),
    HealthData(Result<HealthImport, String>),
    BleAdvertisements(Vec<BleRawAdvertisement>),
    PermissionsStatus {
        health_granted: bool,
        ble_granted: bool,
        hc_available: bool,
    },
    SafeInsets {
        top: f32,
        bottom: f32,
    },
    ClipboardPasted {
        target: ClipboardTarget,
        text: String,
    },
}

pub struct PlatformBridge {
    tx: Sender<PlatformEvent>,
    rx: Receiver<PlatformEvent>,
    pub scanning: bool,
}

impl Default for PlatformBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformBridge {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            scanning: false,
        }
    }

    pub fn poll(&self) -> Vec<PlatformEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            out.push(ev);
        }
        out
    }

    pub fn storage_path() -> PathBuf {
        #[cfg(target_os = "android")]
        {
            if let Some(dir) = android_impl::files_dir() {
                return dir.join("health_hub_store.json");
            }
            PathBuf::from("/data/data/com.example.androidapp/files/health_hub_store.json")
        }
        #[cfg(not(target_os = "android"))]
        {
            let base = std::env::var_os("LOCALAPPDATA")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("HealthHub").join("health_hub_store.json")
        }
    }

    pub fn query_safe_insets(&self) {
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Ok(ev) = android_impl::query_safe_insets() {
                    let _ = tx.send(ev);
                }
            });
        }
    }

    pub fn paste_from_system(&self, target: ClipboardTarget) {
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || match android_impl::call_string("getClipboardText") {
                Ok(text) => {
                    let _ = tx.send(PlatformEvent::ClipboardPasted { target, text });
                }
                Err(e) => {
                    let _ = tx.send(PlatformEvent::Status(format!("Clipboard error: {e}")));
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = target;
            let _ = self.tx.send(PlatformEvent::Status(
                "On desktop, press Ctrl+V inside the text box (or click Paste on Android)."
                    .to_string(),
            ));
        }
    }

    pub fn copy_to_system(&self, text: String) {
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                match android_impl::call_with_string("setClipboardText", &text) {
                    Ok(()) => {
                        let _ = tx.send(PlatformEvent::Status(
                            "Copied to Android clipboard.".to_string(),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(PlatformEvent::Status(format!("Copy failed: {e}")));
                    }
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = text;
        }
    }

    pub fn request_permissions(&self) {
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                match android_impl::call_void("requestPermissions") {
                    Ok(()) => {
                        let _ = tx.send(PlatformEvent::Status(
                            "Opened permissions screen…".to_string(),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(PlatformEvent::Status(format!(
                            "Permission request error: {e}"
                        )));
                    }
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = self.tx.send(PlatformEvent::PermissionsStatus {
                health_granted: true,
                ble_granted: true,
                hc_available: true,
            });
            let _ = self.tx.send(PlatformEvent::Status(
                "Desktop mode: permissions simulated as granted.".to_string(),
            ));
        }
    }

    pub fn check_permissions(&self) {
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Ok(val) = android_impl::check_permissions_json() {
                    let _ = tx.send(val);
                }
                if let Ok(insets) = android_impl::query_safe_insets() {
                    let _ = tx.send(insets);
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = self.tx.send(PlatformEvent::PermissionsStatus {
                health_granted: true,
                ble_granted: true,
                hc_available: true,
            });
        }
    }

    pub fn sync_health_connect(&self, days_back: i64) {
        let tx = self.tx.clone();
        #[cfg(target_os = "android")]
        {
            std::thread::spawn(move || {
                let _ = tx.send(PlatformEvent::Status(format!(
                    "Reading last {days_back} days from Health Connect…"
                )));
                let now_ms = Local::now().timestamp_millis();
                let start_ms = now_ms - days_back * 86_400_000;
                let res = android_impl::read_health_json(start_ms, now_ms).and_then(|json| {
                    serde_json::from_str::<HealthImport>(&json)
                        .map_err(|e| format!("JSON parse error: {e}"))
                });
                let _ = tx.send(PlatformEvent::HealthData(res));
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = days_back;
            let _ = tx.send(PlatformEvent::Status(
                "Desktop mode: use 'Load Demo Data' to populate sample Fitbit Air + S400 data."
                    .to_string(),
            ));
        }
    }

    pub fn set_ble_scanning(&mut self, enable: bool) {
        self.scanning = enable;
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let method = if enable {
                    "startScaleScan"
                } else {
                    "stopScaleScan"
                };
                match android_impl::call_void(method) {
                    Ok(()) => {
                        let msg = if enable {
                            "Scanning for Xiaomi S400 — step on the scale barefoot."
                        } else {
                            "BLE scan stopped."
                        };
                        let _ = tx.send(PlatformEvent::Status(msg.to_string()));
                    }
                    Err(e) => {
                        let _ = tx.send(PlatformEvent::Status(format!("BLE scan error: {e}")));
                    }
                }
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            let msg = if enable {
                "Desktop mode: BLE scan active. Tap 'Simulate S400' to test."
            } else {
                "BLE scan stopped."
            };
            let _ = self.tx.send(PlatformEvent::Status(msg.to_string()));
        }
    }

    pub fn poll_ble_packets(&self) {
        if !self.scanning {
            return;
        }
        #[cfg(target_os = "android")]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Ok(ads) = android_impl::poll_ble_events() {
                    if !ads.is_empty() {
                        let _ = tx.send(PlatformEvent::BleAdvertisements(ads));
                    }
                }
            });
        }
    }

    /// Feeds real S400 test vectors through the BLE pipeline so the user can test
    /// MiBeacon AES-CCM decryption + dual-packet assembly on desktop or phone.
    pub fn simulate_s400_broadcast(&self) -> (&'static str, &'static str) {
        const MAC: &str = "8C:D0:B2:F6:BE:EF";
        const KEY: &str = "0728974d657a4b60964c1b1677f35f7c";
        const PKT1_HEX: &str = "4859d53b0abc078ff2348c844138e930220000009e538599";
        const PKT2_HEX: &str = "4859d53b0bd6ef0b25db72785e7e2f46d6000000d8642df6";

        let now = Local::now().timestamp_millis();
        let _ = self.tx.send(PlatformEvent::BleAdvertisements(vec![
            BleRawAdvertisement {
                mac: MAC.to_string(),
                rssi: -58,
                data_hex: PKT1_HEX.to_string(),
                time_ms: now,
            },
            BleRawAdvertisement {
                mac: MAC.to_string(),
                rssi: -59,
                data_hex: PKT2_HEX.to_string(),
                time_ms: now + 400,
            },
        ]));
        (MAC, KEY)
    }
}

/// Populate realistic 35-day Fitbit Air + Xiaomi S400 data so the user can immediately
/// explore all charts, metrics, and rule-based recommendations.
pub fn populate_demo_data(store: &mut Store) {
    let today: NaiveDate = Local::now().date_naive();
    if store.profile.is_none() {
        store.profile = Some(crate::model::UserProfile::default());
    }

    for i in (0..35).rev() {
        let d = today - Duration::days(i);
        let phase = (i as f64 * 0.7).sin();
        let weekend = matches!(
            chrono::Datelike::weekday(&d),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        );
        let steps = (8_400.0 + phase * 2_400.0 + if weekend { 1_500.0 } else { 0.0 }) as u64;
        let distance_m = steps as f64 * 0.76;
        let active_kcal = steps as f64 * 0.045 + if i % 3 == 0 { 220.0 } else { 60.0 };
        let exercise_min = if i % 2 == 0 { 35.0 } else { 15.0 };
        let resting_hr = 58.0 - (35 - i) as f64 * 0.06 + (phase * 1.5);
        let hrv = 48.0 + (35 - i) as f64 * 0.15 - (phase * 4.0);

        store.days.insert(
            d,
            DayMetrics {
                date: d,
                steps: Some(steps),
                distance_m: Some(distance_m),
                active_kcal: Some(active_kcal),
                total_kcal: Some(1_720.0 + active_kcal),
                exercise_min: Some(exercise_min),
                avg_hr: Some(71.0 + phase * 3.0),
                min_hr: Some(50.0),
                max_hr: Some(142.0 + if i % 3 == 0 { 18.0 } else { 0.0 }),
                resting_hr: Some(resting_hr),
                hrv_rmssd: Some(hrv),
                spo2_avg: Some(96.8),
                spo2_min: Some(94.0),
                resp_rate: Some(14.6),
            },
        );

        if i > 0 {
            let sleep_min = 445.0 + phase * 35.0;
            let end_ms = d
                .and_hms_opt(7, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp_millis();
            let start_ms = end_ms - (sleep_min as i64) * 60_000;
            let total_ms = end_ms - start_ms;
            let deep_ms = (total_ms as f64 * 0.18) as i64;
            let rem_ms = (total_ms as f64 * 0.22) as i64;
            let awake_ms = (total_ms as f64 * 0.06) as i64;
            let light_ms = total_ms - deep_ms - rem_ms - awake_ms;

            let s1 = start_ms + deep_ms;
            let s2 = s1 + light_ms;
            let s3 = s2 + rem_ms;

            let bedtime_min = 23 * 60 + ((phase * 20.0) as i32);
            if !store.sleep.iter().any(|s| s.date == d) {
                store.sleep.push(SleepSession {
                    start_ms,
                    end_ms,
                    date: d,
                    bedtime_min,
                    stages: vec![
                        SleepStage {
                            start_ms,
                            end_ms: s1,
                            kind: StageKind::Deep,
                        },
                        SleepStage {
                            start_ms: s1,
                            end_ms: s2,
                            kind: StageKind::Light,
                        },
                        SleepStage {
                            start_ms: s2,
                            end_ms: s3,
                            kind: StageKind::Rem,
                        },
                        SleepStage {
                            start_ms: s3,
                            end_ms,
                            kind: StageKind::Awake,
                        },
                    ],
                    source: "Google Fitbit Air (Demo)".to_string(),
                });
            }
        }

        if i % 3 == 0 {
            let t_ms = d
                .and_hms_opt(7, 15, 0)
                .unwrap()
                .and_utc()
                .timestamp_millis();
            let progress = (35 - i) as f32 / 35.0;
            let weight = 78.4 - progress * 1.6 + (phase as f32 * 0.2);
            let imp_low = 525.0 - progress * 12.0;
            let imp_high = 480.0 - progress * 10.0;
            store.add_measurement(ScaleMeasurement {
                time_ms: t_ms,
                weight_kg: (weight * 10.0).round() / 10.0,
                impedance_low: Some(imp_low),
                impedance_high: Some(imp_high),
                heart_rate: Some(64),
                profile_id: Some(1),
                reported_body_fat: None,
                source: MeasurementSource::ScaleBle,
            });
        }
    }
    store.sleep.sort_by_key(|s| s.start_ms);
    store.last_sync_ms = Some(Local::now().timestamp_millis());
}

#[cfg(target_os = "android")]
pub mod android_impl {
    use super::{BleRawAdvertisement, PlatformEvent};
    use jni::objects::{JObject, JString, JValue};
    use jni::JavaVM;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static VM: OnceLock<JavaVM> = OnceLock::new();
    static ACTIVITY: OnceLock<jni::objects::GlobalRef> = OnceLock::new();
    static FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

    pub fn init_from_android_app(app: &winit::platform::android::activity::AndroidApp) {
        unsafe {
            let vm_ptr = app.vm_as_ptr() as *mut jni::sys::JavaVM;
            if let Ok(vm) = JavaVM::from_raw(vm_ptr) {
                if let Ok(env) = vm.attach_current_thread() {
                    let act_obj = JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject);
                    if let Ok(gref) = env.new_global_ref(&act_obj) {
                        let _ = ACTIVITY.set(gref);
                    }
                }
                let _ = VM.set(vm);
            }
        }
        if let Some(p) = app.internal_data_path() {
            let _ = FILES_DIR.set(p);
        }
    }

    pub fn files_dir() -> Option<PathBuf> {
        FILES_DIR.get().cloned()
    }

    fn with_activity<F, R>(f: F) -> Result<R, String>
    where
        F: FnOnce(&mut jni::JNIEnv, &JObject) -> Result<R, String>,
    {
        let vm = VM.get().ok_or("JavaVM not initialized")?;
        let act = ACTIVITY.get().ok_or("Activity not initialized")?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        f(&mut env, act.as_obj())
    }

    pub fn call_void(method: &str) -> Result<(), String> {
        with_activity(|env, act| {
            env.call_method(act, method, "()V", &[]).map_err(|e| {
                let _ = env.exception_clear();
                format!("{method} failed: {e}")
            })?;
            Ok(())
        })
    }

    pub fn call_with_string(method: &str, arg: &str) -> Result<(), String> {
        with_activity(|env, act| {
            let jarg = env.new_string(arg).map_err(|e| e.to_string())?;
            env.call_method(
                act,
                method,
                "(Ljava/lang/String;)V",
                &[JValue::Object(&JObject::from(jarg))],
            )
            .map_err(|e| {
                let _ = env.exception_clear();
                format!("{method} failed: {e}")
            })?;
            Ok(())
        })
    }

    pub fn call_string(method: &str) -> Result<String, String> {
        with_activity(|env, act| {
            let res = env
                .call_method(act, method, "()Ljava/lang/String;", &[])
                .map_err(|e| {
                    let _ = env.exception_clear();
                    format!("{method} failed: {e}")
                })?
                .l()
                .map_err(|e| e.to_string())?;
            let jstr = JString::from(res);
            let rust_str: String = env.get_string(&jstr).map_err(|e| e.to_string())?.into();
            Ok(rust_str)
        })
    }

    pub fn read_health_json(start_ms: i64, end_ms: i64) -> Result<String, String> {
        with_activity(|env, act| {
            let res = env
                .call_method(
                    act,
                    "readHealthJson",
                    "(JJ)Ljava/lang/String;",
                    &[JValue::Long(start_ms), JValue::Long(end_ms)],
                )
                .map_err(|e| {
                    let _ = env.exception_clear();
                    format!("readHealthJson failed: {e}")
                })?
                .l()
                .map_err(|e| e.to_string())?;
            let jstr = JString::from(res);
            let rust_str: String = env.get_string(&jstr).map_err(|e| e.to_string())?.into();
            Ok(rust_str)
        })
    }

    pub fn check_permissions_json() -> Result<PlatformEvent, String> {
        #[derive(serde::Deserialize)]
        struct PermResp {
            #[serde(default)]
            health_granted: bool,
            #[serde(default)]
            ble_granted: bool,
            #[serde(default)]
            hc_available: bool,
        }
        let s = call_string("getPermissionsStatusJson")?;
        let p: PermResp = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(PlatformEvent::PermissionsStatus {
            health_granted: p.health_granted,
            ble_granted: p.ble_granted,
            hc_available: p.hc_available,
        })
    }

    pub fn query_safe_insets() -> Result<PlatformEvent, String> {
        #[derive(serde::Deserialize)]
        struct InsetsResp {
            #[serde(default)]
            top_dp: f32,
            #[serde(default)]
            bottom_dp: f32,
        }
        let s = call_string("getSafeInsetsJson")?;
        let r: InsetsResp = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(PlatformEvent::SafeInsets {
            top: r.top_dp.max(44.0),
            bottom: r.bottom_dp.max(24.0),
        })
    }

    pub fn poll_ble_events() -> Result<Vec<BleRawAdvertisement>, String> {
        #[derive(serde::Deserialize)]
        struct RawAd {
            mac: String,
            rssi: i32,
            data_hex: String,
            time_ms: i64,
        }
        let s = call_string("pollBleEventsJson")?;
        if s.trim().is_empty() || s.trim() == "[]" {
            return Ok(vec![]);
        }
        let raw: Vec<RawAd> = serde_json::from_str(&s).map_err(|e| e.to_string())?;
        Ok(raw
            .into_iter()
            .map(|r| BleRawAdvertisement {
                mac: r.mac,
                rssi: r.rssi,
                data_hex: r.data_hex,
                time_ms: r.time_ms,
            })
            .collect())
    }
}
