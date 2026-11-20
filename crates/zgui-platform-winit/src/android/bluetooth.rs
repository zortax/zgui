//! Bluetooth Low Energy scans and connections. Needs
//! [`BLUETOOTH_SCAN`](super::permission::BLUETOOTH_SCAN) and
//! [`BLUETOOTH_CONNECT`](super::permission::BLUETOOTH_CONNECT).

use super::{Guard, jvm};

/// A device a scan found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Hardware address; [`connect`] takes it.
    pub address: String,
    /// Advertised name.
    pub name: Option<String>,
    /// Signal strength in dBm.
    pub rssi: i32,
}

/// Sends each advertisement to `callback` until the guard drops.
///
/// # Errors
///
/// Bluetooth is off or absent, permission is missing, or the JNI call failed.
pub fn scan(mut callback: impl FnMut(Device) + Send + 'static) -> Result<Guard, String> {
    jvm::many("ble.scan", &[], move |reply| {
        if reply.code == jvm::OK {
            callback(Device {
                address: reply.get(0).to_owned(),
                name: Some(reply.get(1).to_owned()).filter(|name| !name.is_empty()),
                rssi: reply.get(2).parse().unwrap_or(0),
            });
        }
    })
}

/// State of a GATT connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gatt {
    /// Connected; service discovery follows.
    Connected,
    /// Disconnected with Android's GATT status code.
    Disconnected(i32),
    /// UUIDs of the device's services.
    Services(Vec<String>),
}

/// Connects to the device at `address` and discovers its services. Dropping the guard
/// disconnects.
///
/// # Errors
///
/// Bluetooth is off or absent, permission is missing, or the JNI call failed.
pub fn connect(
    address: &str,
    mut callback: impl FnMut(Gatt) + Send + 'static,
) -> Result<Guard, String> {
    jvm::many("ble.connect", &[address], move |reply| {
        if reply.code != jvm::OK {
            return;
        }
        callback(match reply.get(0) {
            "connected" => Gatt::Connected,
            "services" => Gatt::Services(reply.text[1..].to_vec()),
            _ => Gatt::Disconnected(reply.get(1).parse().unwrap_or(-1)),
        });
    })
}
