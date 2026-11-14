//! Android services for ZGUI applications, with no Java code in the application.
//!
//! The library ships its Java classes as one dex file, `android/zgui.dex` in this crate.
//! Packaging copies it into the APK as `classes.dex`; the manifest names the classes an
//! application uses:
//!
//! | Class | Manifest entry | Enables |
//! |---|---|---|
//! | `dev.zgui.android.ZguiActivity` | `<activity>` with `android.app.lib_name` | input with TalkBack exploration, [`intent`], [`system::on_window_mode`] |
//! | `dev.zgui.android.ZguiService` | `<service>` with a `foregroundServiceType` | [`background::start_service`] |
//! | `dev.zgui.android.ZguiReceiver` | `<receiver>`, `BOOT_COMPLETED` filter | alarms, notification actions, restart, USB grants |
//! | `dev.zgui.android.ZguiJobs` | `<service>` with `BIND_JOB_SERVICE` | [`background::schedule_job`] |
//! | `dev.zgui.android.ZguiFiles` | `<provider>`, authority `PACKAGE.zgui.files`, `grantUriPermissions` | [`document::share_file`], [`media::capture_photo`] |
//! | `dev.zgui.android.ZguiWidget` | `<receiver>` with the widget filter and provider XML | [`system::pin_widget`] |
//!
//! A stock `NativeActivity` also works; then [`intent`] and window mode events are absent.
//! Work in a process without an activity (services, jobs, alarms, notification actions) runs
//! the handler that [`android_background!`](crate::android_background) defines.
//!
//! Callbacks run on Android threads. Use `zgui::ui().post` to reach UI state.

pub use winit::platform::android::activity::AndroidApp;

pub mod background;
pub mod bluetooth;
pub mod document;
mod input;
pub mod intent;
pub(crate) mod jvm;
pub mod location;
pub mod media;
pub mod nfc;
pub mod notification;
pub mod permission;
pub mod power;
pub mod security;
pub mod system;
pub mod usb;

/// Ends an ongoing operation, such as a location watch or a scan, when dropped.
#[derive(Debug)]
#[must_use = "dropping the guard ends the operation"]
pub struct Guard(u64);

impl Drop for Guard {
    fn drop(&mut self) {
        jvm::forget(self.0);
        if let Err(error) = jvm::call("cancel", self.0, &[]) {
            tracing::warn!(%error, "Android operation did not stop");
        }
    }
}

/// Items the [`android_background!`](crate::android_background) macro expands to.
#[doc(hidden)]
pub mod __private {
    pub use super::background::dispatch;
    pub use jni::EnvUnowned;
    pub use jni::objects::{JClass, JObject, JObjectArray, JString};
}

/// Forwards activity-owned Java input to the Android surface.
///
/// Use only when the activity calls `Window.takeInputQueue(null)` before attaching its window;
/// forwarding events while winit owns the native queue would deliver input twice. Pointer
/// coordinates must be CSS pixels. The backend assigns its own clock timestamp.
pub fn dispatch_input(event: zgui_platform::SurfaceEvent) {
    crate::android_host::dispatch_input(event);
}

/// Posts a system notification after `delay`. Tapping it returns to the application.
///
/// Android 13 and newer ask the user for permission on first use, then post.
/// The delay runs on the Android main thread, so it continues while the application is in
/// the background.
///
/// # Errors
///
/// `ZguiHost` did not load, or the JNI call failed.
pub fn notify(title: &str, text: &str, delay: std::time::Duration) -> Result<(), String> {
    crate::android_host::notify(title, text, delay)
}

/// Sets the font file the native text editor measures with, so Android's selection handles
/// and caret positions match the drawn text. Only the first call has an effect; without
/// one, the editor uses the system font. [`run_android`](crate::app::run_android) reads it.
pub fn editor_font(data: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync>) {
    crate::android_host::editor_font(data);
}

/// Haptic feedback kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Haptic {
    /// A light tick, such as a toggle or a slider step.
    Tick,
    /// A long press.
    LongPress,
    /// A completed action.
    Confirm,
}

/// Plays haptic feedback. The system haptics setting can turn it off.
pub fn haptic(kind: Haptic) {
    crate::android_host::haptic(match kind {
        Haptic::Tick => 0,
        Haptic::LongPress => 1,
        Haptic::Confirm => 2,
    });
}

/// Tells Android which theme the application shows, so the status and navigation bar icons
/// contrast with it: `Some(true)` dark, `Some(false)` light, `None` follows the system theme.
pub fn system_bar_theme(dark: Option<bool>) {
    crate::android_host::bar_theme(dark);
}

/// Opens Android's share sheet with `text`.
pub fn share(text: &str) {
    crate::android_host::share(text);
}

/// Opens `url` in the application that handles it, such as a browser.
pub fn open_url(url: &str) {
    crate::android_host::open_url(url);
}

/// Sends gravity tilt to `callback`, in units of g: x to the right, y up the screen.
/// `None` stops the sensor. Reports pause while the application is in the background.
///
/// `callback` runs on Android's main thread; use `zgui::ui().post` to reach UI state.
pub fn on_tilt(callback: Option<Box<dyn Fn(f32, f32) + Send>>) {
    crate::android_host::on_tilt(callback);
}

/// Opens the system photo picker. `callback` receives a PNG copy of the picked image, at most
/// 1600 pixels on its longer side, or `None` when the user cancels.
///
/// `callback` runs on a worker thread; use `zgui::ui().post` to reach UI state.
pub fn pick_image(callback: impl FnOnce(Option<std::path::PathBuf>) + Send + 'static) {
    crate::android_host::pick_image(Box::new(callback));
}
