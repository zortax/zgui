//! Android component lab. Build and device instructions: `examples/android.md`.

#[path = "android/controls.rs"]
mod controls;
#[path = "android/app.rs"]
mod lab;
#[path = "android/tasks.rs"]
mod tasks;
#[path = "android/visuals.rs"]
mod visuals;

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
    let fonts = Fonts::shipped_only();
    fonts
        .register(
            Arc::new(*include_bytes!(
                "../crates/zgui-text-parley/tests/fonts/NotoSans-Regular.ttf"
            )),
            Some("Demo"),
        )
        .expect("bundled demo font is valid");
    app()
        .with_title("ZGUI Mobile Lab")
        .with_application_id("dev.zgui.counter")
        .with_fonts(fonts)
        .with_stylesheet(include_str!("android/style.css"))
        // Application context survives surface loss and document reconstruction.
        .with_context(|| provide_local_context(lab::State::new()))
        .run_on(driver, || view! { lab::Lab() })
        .expect("mobile lab runs");
}

/// NativeActivity entry point, invoked by android-activity.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub fn android_main(android_app: zgui::app::AndroidApp) {
    run(move |handler| zgui::app::run_android(android_app, handler));
}

#[cfg(not(target_os = "android"))]
const _: fn() = || run(zgui::app::desktop());
