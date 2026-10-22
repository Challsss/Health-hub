#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    androidapp::run_desktop()
}

#[cfg(target_os = "android")]
fn main() {}
