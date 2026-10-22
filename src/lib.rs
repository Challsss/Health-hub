pub mod app;
pub mod bodycomp;
pub mod mibeacon;
pub mod model;
pub mod platform;
pub mod reports;
pub mod store;

use app::HealthHubApp;

/// Entry point for Desktop (`cargo run`).
#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> eframe::Result {
    use eframe::egui;
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([480.0, 840.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Health Hub — Fitbit Air + Xiaomi S400",
        options,
        Box::new(|cc| Ok(Box::new(HealthHubApp::new(cc)))),
    )
}

/// Entry point for Android (`NativeActivity`).
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("androidapp"),
    );

    platform::android_impl::init_from_android_app(&app);

    let options = eframe::NativeOptions {
        event_loop_builder: Some(Box::new(move |builder| {
            use winit::platform::android::EventLoopBuilderExtAndroid;
            builder.with_android_app(app);
        })),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "Health Hub",
        options,
        Box::new(|cc| Ok(Box::new(HealthHubApp::new(cc)))),
    ) {
        log::error!("Application exited with error: {err}");
    }
}
