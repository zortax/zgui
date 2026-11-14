//! Biometric authentication, hardware-backed encryption and account selection.

use super::jvm;

/// Result of [`authenticate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Auth {
    /// The user authenticated.
    Succeeded,
    /// The user cancelled.
    Cancelled,
    /// Authentication failed, for example after too many attempts.
    Failed(String),
}

/// Shows Android's biometric prompt with `title`, `subtitle` (may be empty) and a cancel
/// button labelled `cancel`. Needs `USE_BIOMETRIC` in the manifest and Android 9.
///
/// # Errors
///
/// No activity is visible in this process, Android is older than 9, or the JNI call failed.
pub fn authenticate(
    title: &str,
    subtitle: &str,
    cancel: &str,
    callback: impl FnOnce(Auth) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.biometric", &[title, subtitle, cancel], move |reply| {
        callback(match reply.result() {
            Ok(Some(_)) => Auth::Succeeded,
            Ok(None) => Auth::Cancelled,
            Err(error) => Auth::Failed(error),
        });
    })
}

/// Encrypts `plain` with AES-GCM under Android Keystore key `alias`, which stays in secure
/// hardware where the device has it. Creates the key on first use. The result holds the nonce.
///
/// # Errors
///
/// The keystore refused the operation.
pub fn encrypt(alias: &str, plain: &[u8]) -> Result<Vec<u8>, String> {
    jvm::bytes("keystore.encrypt", &[alias], plain)
}

/// Decrypts the output of [`encrypt`] with key `alias`.
///
/// # Errors
///
/// The key does not exist, or the data is not authentic.
pub fn decrypt(alias: &str, sealed: &[u8]) -> Result<Vec<u8>, String> {
    jvm::bytes("keystore.decrypt", &[alias], sealed)
}

/// Deletes Keystore key `alias`.
///
/// # Errors
///
/// The keystore refused the operation.
pub fn delete_key(alias: &str) -> Result<(), String> {
    jvm::call("keystore.delete", 0, &[alias]).map(drop)
}

/// Lets the user pick an account on the device. `callback` receives its name, or `None` when
/// the user cancels.
///
/// # Errors
///
/// No activity is visible in this process, or the JNI call failed.
pub fn choose_account(
    callback: impl FnOnce(Option<String>) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.account", &[], move |reply| {
        callback(
            reply
                .result()
                .ok()
                .flatten()
                .and_then(|t| t.into_iter().next()),
        );
    })
}
