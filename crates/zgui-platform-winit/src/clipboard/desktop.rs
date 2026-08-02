//! The clipboard as the rest of the desktop offers it.

use arboard::Clipboard;
use zgui_platform::{ClipboardError, ClipboardKind};

/// The platform clipboard, over whichever protocol the desktop-wide library speaks.
///
/// It is opened on first use rather than at start-up. Opening it costs a connection and a thread,
/// and an application that never copies anything should pay neither — nor should one fail to start
/// on a machine whose clipboard is unavailable, when everything else about it works.
#[derive(Default)]
pub(crate) struct Desktop {
    /// The connection, once something has asked for it.
    clipboard: Option<Clipboard>,
}

impl Desktop {
    /// The connection, opening it if this is the first request.
    fn connection(&mut self) -> Result<&mut Clipboard, ClipboardError> {
        if self.clipboard.is_none() {
            self.clipboard =
                Some(Clipboard::new().map_err(|error| ClipboardError::Backend(error.to_string()))?);
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| ClipboardError::Backend("the clipboard could not be opened".to_owned()))
    }

    /// What is on `kind`, as text.
    pub(crate) fn text(&mut self, kind: ClipboardKind) -> Result<String, ClipboardError> {
        let clipboard = self.connection()?;
        match read(clipboard, kind) {
            Ok(text) if text.is_empty() => Err(ClipboardError::Empty(kind)),
            Ok(text) => Ok(text),
            Err(error) => Err(translate(error, kind)),
        }
    }

    /// Puts `text` on `kind`, optionally asking clipboard managers not to record it.
    pub(crate) fn set_text(
        &mut self,
        kind: ClipboardKind,
        text: String,
        secret: bool,
    ) -> Result<(), ClipboardError> {
        let clipboard = self.connection()?;
        write(clipboard, kind, text, secret).map_err(|error| translate(error, kind))
    }
}

/// What a refusal from the desktop-wide library means in the contract's vocabulary.
///
/// The distinction that matters is between "there is nothing there" and "this desktop has no such
/// clipboard": the first greys a paste command out and the second hides it, and reporting either
/// as the other is an interface that offers something it can never do.
fn translate(error: arboard::Error, kind: ClipboardKind) -> ClipboardError {
    match error {
        arboard::Error::ContentNotAvailable => ClipboardError::Empty(kind),
        arboard::Error::ClipboardNotSupported => ClipboardError::UnsupportedKind(kind),
        arboard::Error::ClipboardOccupied => ClipboardError::TimedOut,
        other => ClipboardError::Backend(other.to_string()),
    }
}

/// Reads `kind`, on a desktop that has more than one clipboard.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn read(clipboard: &mut Clipboard, kind: ClipboardKind) -> Result<String, arboard::Error> {
    use arboard::GetExtLinux;

    clipboard.get().clipboard(selection(kind)).text()
}

/// Reads the one clipboard this desktop has.
#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn read(clipboard: &mut Clipboard, kind: ClipboardKind) -> Result<String, arboard::Error> {
    if matches!(kind, ClipboardKind::Primary) {
        return Err(arboard::Error::ClipboardNotSupported);
    }
    clipboard.get_text()
}

/// Writes to `kind`, on a desktop that has more than one clipboard.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn write(
    clipboard: &mut Clipboard,
    kind: ClipboardKind,
    text: String,
    secret: bool,
) -> Result<(), arboard::Error> {
    use arboard::SetExtLinux;

    let set = clipboard.set().clipboard(selection(kind));
    // A request rather than a guarantee: a desktop that ignores the hint still records the value,
    // so a password field is told this is a mitigation and not a promise.
    if secret {
        set.exclude_from_history().text(text)
    } else {
        set.text(text)
    }
}

/// Writes to the one clipboard this desktop has.
#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn write(
    clipboard: &mut Clipboard,
    kind: ClipboardKind,
    text: String,
    _secret: bool,
) -> Result<(), arboard::Error> {
    if matches!(kind, ClipboardKind::Primary) {
        return Err(arboard::Error::ClipboardNotSupported);
    }
    clipboard.set_text(text)
}

/// Which of the desktop's selections a clipboard kind names.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
const fn selection(kind: ClipboardKind) -> arboard::LinuxClipboardKind {
    match kind {
        ClipboardKind::Primary => arboard::LinuxClipboardKind::Primary,
        _ => arboard::LinuxClipboardKind::Clipboard,
    }
}
