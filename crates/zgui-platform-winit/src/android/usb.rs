//! USB devices attached to the phone in host mode.

use super::jvm;
use std::os::fd::OwnedFd;

/// An attached device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Device path; the other functions take it.
    pub name: String,
    /// USB vendor id.
    pub vendor_id: u16,
    /// USB product id.
    pub product_id: u16,
    /// Number of interfaces.
    pub interfaces: u32,
    /// Whether the user allowed access.
    pub permitted: bool,
}

/// Attached devices.
pub fn devices() -> Vec<Device> {
    let fields = jvm::call("usb.list", 0, &[]).unwrap_or_default();
    fields
        .chunks_exact(5)
        .map(|item| Device {
            name: item[0].clone(),
            vendor_id: item[1].parse().unwrap_or(0),
            product_id: item[2].parse().unwrap_or(0),
            interfaces: item[3].parse().unwrap_or(0),
            permitted: item[4] == "1",
        })
        .collect()
}

/// Asks the user for access to device `name`. `callback` receives `true` when granted.
///
/// # Errors
///
/// The device is not attached, or the JNI call failed.
pub fn request_permission(
    name: &str,
    callback: impl FnOnce(bool) + Send + 'static,
) -> Result<(), String> {
    jvm::once("usb.permission", &[name], move |reply| {
        callback(reply.code == jvm::OK && reply.get(0) == "1");
    })
}

/// Opens device `name` and returns its usbfs file descriptor, for a USB library such as
/// `rusb` (`wrap_sys_device`).
///
/// # Errors
///
/// Access is not granted, the device is not attached, or the JNI call failed.
pub fn open(name: &str) -> Result<OwnedFd, String> {
    Ok(jvm::file(&jvm::call_one("usb.open", &[name])?)?.into())
}
