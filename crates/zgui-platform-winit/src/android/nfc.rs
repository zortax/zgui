//! NFC tag reading while the activity is visible.

use super::{Guard, jvm};

/// A tag the reader found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// Tag id.
    pub id: Vec<u8>,
    /// Android technology class names, such as `android.nfc.tech.NfcA`.
    pub techs: Vec<String>,
    /// NDEF message bytes the tag holds.
    pub ndef: Option<Vec<u8>>,
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .filter_map(|i| u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// Sends each tag to `callback` until the guard drops. Reading pauses while the activity is
/// hidden.
///
/// # Errors
///
/// NFC is off or absent, no activity is visible in this process, or the JNI call failed.
pub fn read(mut callback: impl FnMut(Tag) + Send + 'static) -> Result<Guard, String> {
    jvm::many("host.nfc", &[], move |reply| {
        if reply.code == jvm::OK {
            callback(Tag {
                id: hex(reply.get(0)),
                techs: reply.text.get(1..).unwrap_or_default().to_vec(),
                ndef: reply.bytes,
            });
        }
    })
}
