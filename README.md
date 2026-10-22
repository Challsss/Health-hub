# Health Hub

A unified, privacy-first **Android + Desktop** health dashboard and recommendation app built in **Rust** (`egui` + `eframe` + `wgpu`) with a lightweight Android **Kotlin** bridge for **Health Connect** and **Bluetooth Low Energy (BLE)**.

Health Hub combines daily activity, sleep, and heart recovery metrics from a **Google Fitbit Air** with dual-frequency bioelectrical impedance body composition measurements from a **Xiaomi Body Composition Scale S400 (`MJTZC01YM`)** in a single, clean mobile interface.

---

## Key Features

### 1. Google Fitbit Air (via Android Health Connect)
- **Daily Activity**: Steps, distance (km), active & total calories burned, and exercise duration.
- **Heart & Recovery**: Average/min/max heart rate, resting heart rate (RHR), heart rate variability (HRV RMSSD), overnight SpO₂, and respiratory rate.
- **Sleep & Recovery**: Full sleep stage breakdown (**Deep**, **REM**, **Light**, **Awake**), total sleep duration, and bedtime consistency tracking.

### 2. Xiaomi Body Composition Scale S400 (`MJTZC01YM`)
- **Passive BLE MiBeacon (`0xFE95`) Scanner**: Listens for S400 advertisements without needing the Xiaomi Home app open.
- **Local AES-CCM Decryption**: Decrypts MiBeacon v4/v5 payloads locally in Rust using your 32-character Xiaomi BLE bindkey (with one-tap Android clipboard paste in Settings).
- **Dual-Frequency BIA Calibration (`50 kHz` + `250 kHz`)**: Combines extracellular (`50 kHz`) and intracellular (`250 kHz`) impedance measurements to compute:
  - Weight & BMI (with ideal weight target)
  - Body Fat % & Fat Mass (calibrated for S400 dual-frequency Siri 2-compartment model)
  - Lean Body Mass & Muscle Mass (kg)
  - Visceral Fat Level (BIA Fat Mass Index model calibrated to Xiaomi Home)
  - Body Water %, Bone Mass (kg), Protein %, BMR (Katch-McArdle kcal/day), and Metabolic Age
- **Manual Weigh-In Logger**: Log weight and optional impedance manually at any time.

### 3. Reports & Rule-Based Recommendation Engine
- **Weekly (7d), Monthly (30d), and Quarterly (90d)** aggregated reports with period-over-period deltas and linear trend slopes.
- **Actionable Insights**: Prioritized rules (`Alert`, `Warning`, `Info`, `Good`) across Activity, Sleep, Heart & Recovery, Body Composition, and cross-metric correlations (e.g., detecting muscle loss during rapid weight loss or correlating short sleep with elevated resting HR / depressed HRV).

### 4. Mobile-First UI (Tuned for High-DPI Tall Screens like Galaxy S25 Ultra)
- **Warm Brown-to-Orange Palette**: Espresso-brown to burnt-orange/amber dark theme (plus a warm latte-to-orange light theme).
- **Safe-Area Insets**: Automatic camera-cutout top padding and gesture-bar bottom padding via Android `WindowInsets`.
- **Touch-Scrubbable Charts**: Clean, uncrowded bar and line charts where touching or sliding across the graphic highlights the data point and displays its full breakdown in a dedicated readout bar **below** the chart (so your finger never covers the values).
- **Custom Vector Icons**: All navigation, header, status, clear, and delete icons are rendered as resolution-independent vector shapes via `egui::Painter`.

---

## Project Structure

```text
├── Cargo.toml                  # Rust crate manifest (cdylib for Android + desktop binary)
├── build-android.ps1           # One-step cargo-ndk + Gradle APK build & install script
├── src/
│   ├── lib.rs                  # Android NativeActivity entry point & module exports
│   ├── main.rs                 # Desktop preview entry point
│   ├── app.rs                  # egui mobile UI, custom vector icons, charts & theme
│   ├── model.rs                # Core data models (DayMetrics, SleepSession, ScaleMeasurement)
│   ├── mibeacon.rs             # Xiaomi MiBeacon v4/v5 AES-CCM decryption & S400 packet assembler
│   ├── bodycomp.rs             # Dual-frequency (50/250 kHz) & single-frequency BIA formulas
│   ├── reports.rs              # Weekly/Monthly/Quarterly aggregation & recommendation engine
│   ├── store.rs                # Local JSON persistence & Health Connect merge logic
│   └── platform.rs             # JNI bridge (Android) & desktop simulation fallback
└── android/                    # Android Gradle project (Kotlin bridge)
    └── app/src/main/java/com/example/androidapp/
        ├── MainActivity.kt         # NativeActivity host, clipboard & WindowInsets bridge
        ├── HealthBridge.kt         # Android Health Connect reader
        ├── BleScaleScanner.kt      # Passive BLE scanner for Xiaomi 0xFE95 broadcasts
        └── PermissionsActivity.kt  # Runtime permissions & Health Connect manager screen
```

---

## Getting Started

### Prerequisites
- **Rust** (`rustup` with `aarch64-linux-android` and `x86_64-linux-android` targets)
- **cargo-ndk** (`cargo install cargo-ndk`)
- **Android SDK & NDK** (Android SDK Platform 35/36, NDK `27.0.12077973`)
- **JDK 17+** (e.g., Android Studio bundled JBR)

### Run on Desktop (Windows / macOS / Linux)
```powershell
cargo run
```

### Run Unit Tests
```powershell
cargo test
```

### Build & Install on Android Phone
Connect your Android phone via USB with **USB Debugging** enabled, then run:
```powershell
# Build debug APK:
.\build-android.ps1

# Build, install, and launch on connected device:
.\build-android.ps1 -Install
```
The built APK is generated at `android/app/build/outputs/apk/debug/app-debug.apk`.

---

## Privacy & Security
- **100% Local Storage**: All Health Connect records and Xiaomi S400 measurements are stored exclusively on your device in the app's private storage (`health_hub_store.json`).
- **No Cloud or Telemetry**: The app does not request the `INTERNET` permission on Android and never transmits your health data or BLE bindkey over the network.
