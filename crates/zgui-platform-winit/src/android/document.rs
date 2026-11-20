//! Documents through Android's file picker, and files shared with other applications.

use super::jvm;
use std::fs::File;
use std::path::{Path, PathBuf};

/// A document the user picked or created.
#[derive(Debug)]
pub struct Document {
    /// Content URI. [`reopen`] opens it again later.
    pub uri: String,
    /// Open file: readable for [`open`], writable for [`create`].
    pub file: File,
}

fn picked(reply: jvm::Reply) -> Result<Option<Document>, String> {
    let Some(text) = reply.result()? else {
        return Ok(None);
    };
    let mut text = text.into_iter();
    let uri = text.next().unwrap_or_default();
    let file = jvm::file(&text.next().unwrap_or_default())?;
    Ok(Some(Document { uri, file }))
}

/// Lets the user pick a document of type `mime`, such as `"*/*"`. `callback` receives `None`
/// when the user cancels. Read access persists, so [`reopen`] works after a restart.
///
/// # Errors
///
/// No activity is visible in this process, or the JNI call failed.
pub fn open(
    mime: &str,
    callback: impl FnOnce(Result<Option<Document>, String>) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.open", &[mime], move |reply| callback(picked(reply)))
}

/// Lets the user create a document with suggested `name` and type `mime`. `callback` receives
/// it open for writing, or `None` when the user cancels.
///
/// # Errors
///
/// No activity is visible in this process, or the JNI call failed.
pub fn create(
    name: &str,
    mime: &str,
    callback: impl FnOnce(Result<Option<Document>, String>) + Send + 'static,
) -> Result<(), String> {
    jvm::once("host.create", &[name, mime], move |reply| {
        callback(picked(reply));
    })
}

/// Opens a content URI from [`open`] or an [`Intent`](super::intent::Intent), for reading or
/// for writing.
///
/// # Errors
///
/// The application has no access, or the document no longer exists.
pub fn reopen(uri: &str, write: bool) -> Result<File, String> {
    jvm::file(&jvm::call_one(
        "content",
        &[uri, if write { "w" } else { "r" }],
    )?)
}

/// Directory whose files [`share_file`] and other applications can reach through `ZguiFiles`.
///
/// # Errors
///
/// The JNI call failed.
pub fn shared_dir() -> Result<PathBuf, String> {
    jvm::call("files", 0, &[])?
        .into_iter()
        .nth(2)
        .map(PathBuf::from)
        .ok_or_else(|| "Android files unavailable".into())
}

/// Opens Android's share sheet with the file at `path`, of type `mime`. A file outside
/// [`shared_dir`] is copied there first.
///
/// # Errors
///
/// The copy or the JNI call failed.
pub fn share_file(path: &Path, mime: &str) -> Result<(), String> {
    let dir = shared_dir()?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("File name required")?;
    if path.parent() != Some(dir.as_path()) {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::copy(path, dir.join(name)).map_err(|e| e.to_string())?;
    }
    jvm::call("share.file", 0, &[name, mime]).map(drop)
}
