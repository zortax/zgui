//! Android component lab. Build and device instructions: `examples/android.md`.

#[path = "android/controls.rs"]
mod controls;
#[path = "android/app.rs"]
mod lab;
#[path = "android/phone.rs"]
mod phone;
#[path = "android/services.rs"]
mod services;
#[path = "android/tasks.rs"]
mod tasks;
#[path = "android/visuals.rs"]
mod visuals;
#[path = "android/widgets.rs"]
mod widgets;

use std::sync::Arc;
use zgui::prelude::*;

fn run(
    driver: impl FnOnce(
        Box<dyn zgui::platform::AppHandler>,
    ) -> Result<(), zgui::platform::PlatformError>,
) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .try_init();
    #[cfg(target_os = "android")]
    let _tokio = zgui_tokio::install().expect("Android lab Tokio runtime starts");
    let fonts = Fonts::shipped_only();
    fonts
        .register(
            Arc::new(*include_bytes!(
                "../crates/zgui-text-parley/tests/fonts/NotoSans-Regular.ttf"
            )),
            Some("Demo"),
        )
        .expect("bundled demo font is valid");
    // Arabic and Hebrew come from the phone, for the right-to-left samples.
    for (file, family) in [
        ("/system/fonts/NotoNaskhArabicUI-Regular.ttf", "Arabic"),
        ("/system/fonts/NotoSansHebrew-Regular.ttf", "Hebrew"),
    ] {
        if let Ok(bytes) = std::fs::read(file) {
            let _ = fonts.register(Arc::new(bytes), Some(family));
        }
    }
    app()
        .with_title("ZGUI Mobile Lab")
        .with_application_id("dev.zgui.counter")
        .with_fonts(fonts)
        .with_stylesheet(include_str!("android/style.css"))
        // Application context survives surface loss and document reconstruction.
        .with_context(|| provide_local_context(lab::State::load()))
        .run_on(driver, || view! { lab::Lab() })
        .expect("mobile lab runs");
}

/// NativeActivity entry point, invoked by android-activity.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub fn android_main(android_app: zgui::app::AndroidApp) {
    // NativeActivity can re-enter before winit sees destruction. Runtime and event loop are
    // process-wide; restart before touching either instead of repeatedly panicking on install.
    static ENTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ENTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        std::process::exit(0);
    }
    if let Some(dir) = android_app.internal_data_path() {
        let _ = lab::STORE.set(dir.join("lab-state.txt"));
    }
    services::init();
    run(move |handler| zgui::app::run_android(android_app, handler));
}

#[cfg(target_os = "android")]
zgui::app::android_background!(services::background);

#[cfg(not(target_os = "android"))]
const _: fn() = || run(zgui::app::desktop());
