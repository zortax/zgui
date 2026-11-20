//! Runtime permissions. The manifest must also declare each permission.

use super::jvm;

/// Precise location.
pub const FINE_LOCATION: &str = "android.permission.ACCESS_FINE_LOCATION";
/// Approximate location.
pub const COARSE_LOCATION: &str = "android.permission.ACCESS_COARSE_LOCATION";
/// Location while no activity is visible, outside a foreground service.
pub const BACKGROUND_LOCATION: &str = "android.permission.ACCESS_BACKGROUND_LOCATION";
/// Notifications, on Android 13 and newer. Granted on older versions.
pub const NOTIFICATIONS: &str = "android.permission.POST_NOTIFICATIONS";
/// Microphone.
pub const RECORD_AUDIO: &str = "android.permission.RECORD_AUDIO";
/// Camera access for the application itself. [`media::capture_photo`](super::media::capture_photo)
/// needs none.
pub const CAMERA: &str = "android.permission.CAMERA";
/// Bluetooth scans. Android 11 and older check precise location instead.
pub const BLUETOOTH_SCAN: &str = "android.permission.BLUETOOTH_SCAN";
/// Bluetooth connections. Granted on Android 11 and older.
pub const BLUETOOTH_CONNECT: &str = "android.permission.BLUETOOTH_CONNECT";

/// Whether the user granted `permission`.
pub fn granted(permission: &str) -> bool {
    jvm::call_one("granted", &[permission]).is_ok_and(|value| value == "1")
}

/// Asks the user for `permissions` that are not granted yet. `callback` receives `true` when
/// all are granted. Android stops asking after the user refuses twice; then
/// [`system::open_settings`](super::system::open_settings) is the way to change them.
///
/// # Errors
///
/// No activity is visible in this process, or the JNI call failed.
pub fn request(
    permissions: &[&str],
    callback: impl FnOnce(bool) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.permissions", permissions, move |reply| {
        callback(reply.code == jvm::OK && reply.get(0) == "1");
    })
}
