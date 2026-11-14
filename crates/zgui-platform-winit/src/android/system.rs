//! System settings, launcher shortcuts and widgets, window modes and device information.

use super::jvm;
use std::path::PathBuf;
use std::sync::Mutex;

/// A system settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settings {
    /// The application's details, with its permissions.
    App,
    /// The application's notification settings.
    Notifications,
    /// The battery optimization list.
    BatteryOptimization,
    /// Location on or off.
    Location,
    /// The exact alarm permission (Android 12+; older versions show [`Settings::App`]).
    Alarms,
}

/// Opens a system settings screen.
///
/// # Errors
///
/// The JNI call failed.
pub fn open_settings(screen: Settings) -> Result<(), String> {
    let name = match screen {
        Settings::App => "app",
        Settings::Notifications => "notifications",
        Settings::BatteryOptimization => "battery",
        Settings::Location => "location",
        Settings::Alarms => "alarms",
    };
    jvm::call("settings", 0, &[name]).map(drop)
}

/// The application's private files and cache directories. Works in any process.
///
/// # Errors
///
/// The JNI call failed.
pub fn dirs() -> Result<(PathBuf, PathBuf), String> {
    let mut dirs = jvm::call("files", 0, &[])?.into_iter().map(PathBuf::from);
    match (dirs.next(), dirs.next()) {
        (Some(files), Some(cache)) => Ok((files, cache)),
        _ => Err("Android files unavailable".into()),
    }
}

/// The user's language and region as a BCP 47 tag, such as `en-US`.
///
/// # Errors
///
/// The JNI call failed.
pub fn locale() -> Result<String, String> {
    jvm::call_one("locale", &[])
}

/// Network state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Network {
    /// The network reached the internet.
    pub validated: bool,
    /// The network may have data limits or costs.
    pub metered: bool,
    /// Wi-Fi.
    pub wifi: bool,
    /// Mobile data.
    pub cellular: bool,
    /// Ethernet.
    pub ethernet: bool,
    /// VPN.
    pub vpn: bool,
}

/// The active network, or `None` when offline. Needs `ACCESS_NETWORK_STATE` in the manifest.
///
/// # Errors
///
/// The JNI call failed.
pub fn network() -> Result<Option<Network>, String> {
    let fields = jvm::call("network", 0, &[])?;
    if fields.is_empty() {
        return Ok(None);
    }
    let flag = |index: usize| fields.get(index).is_some_and(|value| value == "1");
    Ok(Some(Network {
        validated: flag(0),
        metered: flag(1),
        wifi: flag(2),
        cellular: flag(3),
        ethernet: flag(4),
        vpn: flag(5),
    }))
}

/// Asks the launcher to pin a shortcut with `label`. Tapping it opens the application with
/// `payload`.
///
/// # Errors
///
/// The launcher does not support pinned shortcuts.
pub fn pin_shortcut(id: &str, label: &str, payload: &str) -> Result<(), String> {
    jvm::call("shortcut", 0, &[id, label, payload]).map(drop)
}

/// Asks the launcher to add the `ZguiWidget` home screen widget.
///
/// # Errors
///
/// The launcher does not support pinned widgets.
pub fn pin_widget() -> Result<(), String> {
    jvm::call("widget.pin", 0, &[]).map(drop)
}

/// Sets the text of every `ZguiWidget`. The text persists.
///
/// # Errors
///
/// The JNI call failed.
pub fn set_widget_text(text: &str) -> Result<(), String> {
    jvm::call("widget.text", 0, &[text]).map(drop)
}

/// Window state of the activity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowMode {
    /// Split screen or a free-form window.
    pub multi_window: bool,
    /// Picture in picture.
    pub picture_in_picture: bool,
}

fn mode(fields: &[String]) -> WindowMode {
    WindowMode {
        multi_window: fields.first().is_some_and(|value| value == "1"),
        picture_in_picture: fields.get(1).is_some_and(|value| value == "1"),
    }
}

/// Current window state.
///
/// # Errors
///
/// No activity is visible in this process.
pub fn window_mode() -> Result<WindowMode, String> {
    Ok(mode(&jvm::call("host.window", 0, &[])?))
}

type ModeHandler = Box<dyn Fn(WindowMode) + Send>;
static WINDOW: Mutex<Option<ModeHandler>> = Mutex::new(None);

/// Sends window state changes to `callback`. Needs `ZguiActivity`.
pub fn on_window_mode(callback: impl Fn(WindowMode) + Send + 'static) {
    *WINDOW.lock().expect("Android window lock") = Some(Box::new(callback));
}

pub(crate) fn window_changed(fields: &[String]) {
    if let Some(handler) = WINDOW.lock().expect("Android window lock").as_ref() {
        handler(mode(fields));
    }
}

/// Shrinks the activity into a picture-in-picture window of aspect `width`:`height`. The
/// manifest activity needs `android:supportsPictureInPicture="true"`.
///
/// # Errors
///
/// The device has no picture in picture, or no activity is visible in this process.
pub fn enter_picture_in_picture(width: u32, height: u32) -> Result<(), String> {
    jvm::call("host.pip", 0, &[&width.to_string(), &height.to_string()]).map(drop)
}

/// Axes of a game controller's sticks, from -1 to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gamepad {
    /// Left stick, horizontal.
    pub x: f32,
    /// Left stick, vertical.
    pub y: f32,
    /// Right stick, horizontal.
    pub z: f32,
    /// Right stick, vertical.
    pub rz: f32,
}

type GamepadHandler = Box<dyn Fn(Gamepad) + Send>;
static GAMEPAD: Mutex<Option<GamepadHandler>> = Mutex::new(None);

/// Sends controller stick movement to `callback`. Buttons arrive as key events. Needs
/// `ZguiActivity`.
pub fn on_gamepad(callback: impl Fn(Gamepad) + Send + 'static) {
    *GAMEPAD.lock().expect("Android gamepad lock") = Some(Box::new(callback));
}

pub(crate) fn gamepad_moved(fields: &[String]) {
    let axis = |index: usize| {
        fields
            .get(index)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0)
    };
    if let Some(handler) = GAMEPAD.lock().expect("Android gamepad lock").as_ref() {
        handler(Gamepad {
            x: axis(0),
            y: axis(1),
            z: axis(2),
            rz: axis(3),
        });
    }
}

/// A sensor on the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sensor {
    /// Name.
    pub name: String,
    /// Android sensor type constant.
    pub kind: i32,
}

/// Sensors on the device.
pub fn sensors() -> Vec<Sensor> {
    jvm::call("sensors", 0, &[])
        .unwrap_or_default()
        .chunks_exact(2)
        .map(|item| Sensor {
            name: item[0].clone(),
            kind: item[1].parse().unwrap_or(0),
        })
        .collect()
}

/// An input device, such as a keyboard, mouse or controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// Android device id.
    pub id: i32,
    /// Name.
    pub name: String,
    /// Android `InputDevice` source flags.
    pub sources: i32,
}

/// Input devices attached to the device.
pub fn input_devices() -> Vec<InputDevice> {
    jvm::call("input.devices", 0, &[])
        .unwrap_or_default()
        .chunks_exact(3)
        .map(|item| InputDevice {
            id: item[0].parse().unwrap_or(0),
            name: item[1].clone(),
            sources: item[2].parse().unwrap_or(0),
        })
        .collect()
}
