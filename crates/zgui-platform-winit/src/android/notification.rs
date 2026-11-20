//! System notifications with a payload and action buttons.

use super::jvm;

/// A system notification.
///
/// Tapping it opens the application and delivers `payload` through
/// [`intent::on_intent`](super::intent::on_intent). Tapping an action button delivers
/// [`Event::Action`](super::background::Event::Action) to the background handler.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notification {
    /// Id. Posting another notification with the same id replaces this one.
    pub id: i32,
    /// Title.
    pub title: String,
    /// Body text.
    pub text: String,
    /// Delivered with the intent that opens the application.
    pub payload: String,
    /// Ongoing work: the user cannot dismiss it, and it uses a quiet channel.
    pub ongoing: bool,
    /// Action buttons: id, then label. Android shows at most three.
    pub actions: Vec<(String, String)>,
}

impl Notification {
    /// Arguments of the Java notification builder.
    pub(crate) fn args(&self) -> Vec<String> {
        let mut args = vec![
            self.id.to_string(),
            self.title.clone(),
            self.text.clone(),
            self.payload.clone(),
            if self.ongoing { "1" } else { "0" }.to_owned(),
        ];
        for (id, label) in &self.actions {
            args.push(id.clone());
            args.push(label.clone());
        }
        args
    }
}

/// Posts `notification`. Android 13 and newer need
/// [`permission::NOTIFICATIONS`](super::permission::NOTIFICATIONS) first.
///
/// # Errors
///
/// The permission is missing, or the JNI call failed.
pub fn post(notification: &Notification) -> Result<(), String> {
    let args = notification.args();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    jvm::call("notify", 0, &args).map(drop)
}

/// Removes notification `id`.
///
/// # Errors
///
/// The JNI call failed.
pub fn cancel(id: i32) -> Result<(), String> {
    jvm::call("notify.cancel", 0, &[&id.to_string()]).map(drop)
}
