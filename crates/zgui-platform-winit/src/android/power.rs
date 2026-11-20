//! Battery, power saving and wake locks.

use super::{Guard, jvm};
use std::time::Duration;

/// Power state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// The user exempted the application from battery optimization.
    pub unrestricted_battery: bool,
    /// Battery saver is on.
    pub power_save: bool,
    /// The device is in doze.
    pub idle: bool,
    /// The user restricted background work of the application (Android 9+).
    pub background_restricted: bool,
    /// Battery charge in percent.
    pub battery_percent: Option<u8>,
    /// The battery charges.
    pub charging: bool,
}

/// Current power state.
///
/// # Errors
///
/// The JNI call failed.
pub fn status() -> Result<Status, String> {
    let fields = jvm::call("power", 0, &[])?;
    let flag = |index: usize| fields.get(index).is_some_and(|value| value == "1");
    Ok(Status {
        unrestricted_battery: flag(0),
        power_save: flag(1),
        idle: flag(2),
        background_restricted: flag(3),
        battery_percent: fields
            .get(4)
            .and_then(|value| value.parse::<i32>().ok())
            .and_then(|value| u8::try_from(value).ok())
            .filter(|value| *value <= 100),
        charging: flag(5),
    })
}

/// Keeps the CPU running for at most `timeout`, or until the guard drops. Needs `WAKE_LOCK` in
/// the manifest.
///
/// # Errors
///
/// The permission is missing, or the JNI call failed.
pub fn wake_lock(timeout: Duration) -> Result<Guard, String> {
    jvm::many("wake", &[&timeout.as_millis().to_string()], |_| {})
}
