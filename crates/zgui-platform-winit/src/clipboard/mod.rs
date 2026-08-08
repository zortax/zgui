//! The desktop's clipboards, and the two protocols they are reached over.
//!
//! There are two implementations behind one type and the choice is made once, at start-up, from
//! the connection the window system opened. The reason is written down here so that nobody
//! re-enables the configuration that breaks it:
//!
//! * **Under a Wayland compositor** the clipboard is reached over the application's own connection
//!   with the ordinary data-device protocol — the protocol a window is supposed to use. The
//!   desktop-wide library's Wayland support speaks the *data-control* protocol instead, which is
//!   what clipboard managers and screen recorders use: it needs a privileged interface, several
//!   compositors do not offer it at all, and where it is missing every copy silently fails. So it
//!   is compiled out and this arm is taken instead.
//! * **Everywhere else**, including X11, the desktop-wide library is exactly right and is used as
//!   it comes.
//!
//! # Why a read is a request and not a return value
//!
//! The content belongs to another process. That process has to be asked for it, may take as long
//! as it likes to answer, and may never answer at all. [`Clipboard::read`] therefore starts a read
//! and hands back an identifier, and the answer arrives later as a wake carrying that identifier —
//! a shape every desktop can satisfy. [`Clipboard::read_blocking`] is there because on a desktop
//! the answer usually *is* available at once, and threading it through the loop for that case is
//! ceremony.

mod desktop;
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod wayland;

use std::cell::RefCell;
use std::sync::Arc;

use raw_window_handle::HasDisplayHandle;
use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions, WakeReason, Waker,
};

use crate::clipboard::desktop::Desktop;

/// Which implementation a running program ended up with.
enum Backend {
    /// The desktop-wide library, which is right everywhere except under a Wayland compositor.
    Desktop(Desktop),
    /// The compositor's own data device, over the connection the window system already owns.
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    Wayland(wayland::Wayland),
}

/// The desktop's clipboards.
///
/// Only plain text crosses this boundary. That is not an omission to be filled in later but what
/// the contract promises and no more: markup, file lists and images are genuinely absent from some
/// real desktops, and a request for one of them is refused with a reason a caller can fall back
/// from rather than answered with an empty value it cannot tell from a genuinely empty clipboard.
pub struct DesktopClipboard {
    /// Identifiers for reads answered through the loop.
    serials: ClipboardSerials,
    /// The implementation, once the connection has been seen.
    backend: RefCell<Backend>,
    /// Where the answer to a read is delivered.
    waker: RefCell<Option<Arc<dyn Waker>>>,
}

impl Default for DesktopClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopClipboard {
    /// A clipboard that has not yet seen which desktop it is on.
    ///
    /// Until the running loop's connection has been seen this is the desktop-wide library, which is the
    /// right answer everywhere but under a Wayland compositor and is never wrong enough to fail.
    pub fn new() -> Self {
        Self {
            serials: ClipboardSerials::new(),
            backend: RefCell::new(Backend::Desktop(Desktop::default())),
            waker: RefCell::new(None),
        }
    }

    /// Chooses the implementation from the connection the window system opened, once.
    ///
    /// The waker is taken at the same time because a read started through the loop is answered
    /// through the loop, and the answer has to reach the same loop the request came from.
    pub(crate) fn attach(&self, display: &dyn HasDisplayHandle, waker: Arc<dyn Waker>) {
        *self.waker.borrow_mut() = Some(waker);
        self.choose(display);
    }

    /// Picks the Wayland implementation when the connection is a Wayland one.
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    fn choose(&self, display: &dyn HasDisplayHandle) {
        use raw_window_handle::RawDisplayHandle;

        let Ok(handle) = display.display_handle() else {
            return;
        };
        if let RawDisplayHandle::Wayland(wayland) = handle.as_raw() {
            // SAFETY: the pointer comes from the running event loop's own display handle, and the
            // event loop owns that connection for as long as the program runs. This clipboard is
            // stored beside the loop and dropped with it, so the connection outlives it.
            let clipboard = unsafe { wayland::Wayland::new(wayland.display.as_ptr()) };
            *self.backend.borrow_mut() = Backend::Wayland(clipboard);
            tracing::debug!(target: "zgui::platform", "the compositor's own data device is in use");
        }
    }

    /// Keeps the desktop-wide implementation, which is the only one on this platform.
    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    fn choose(&self, display: &dyn HasDisplayHandle) {
        let _ = display;
    }

    /// What is on `kind`, as text.
    fn text(&self, kind: ClipboardKind) -> Result<String, ClipboardError> {
        match &mut *self.backend.borrow_mut() {
            Backend::Desktop(desktop) => desktop.text(kind),
            #[cfg(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            Backend::Wayland(wayland) => wayland.text(kind),
        }
    }

    /// Puts `text` on `kind`.
    fn set_text(
        &self,
        kind: ClipboardKind,
        text: String,
        secret: bool,
    ) -> Result<(), ClipboardError> {
        match &mut *self.backend.borrow_mut() {
            Backend::Desktop(desktop) => desktop.set_text(kind, text, secret),
            #[cfg(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            ))]
            Backend::Wayland(wayland) => {
                wayland.set_text(kind, text);
                Ok(())
            }
        }
    }
}

impl core::fmt::Debug for DesktopClipboard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("DesktopClipboard")
    }
}

impl Clipboard for DesktopClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        let result = self.read_blocking(kind, format);
        // A desktop can answer at once, so the answer is delivered before this returns rather than
        // waiting for something to poll. The shape is still the asynchronous one, because it is the
        // one every platform can satisfy and the one a caller has to be written against.
        if let Some(waker) = self.waker.borrow().as_ref() {
            waker.wake(WakeReason::ClipboardRead { serial, result });
        } else {
            tracing::warn!(
                target: "zgui::platform",
                "a clipboard read was started before the loop could answer it"
            );
        }
        serial
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        if format != ClipboardFormat::Text {
            return Err(ClipboardError::UnsupportedFormat(format));
        }
        self.text(kind).map(|text| ClipboardData::Text(text.into()))
    }

    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        let ClipboardData::Text(text) = data else {
            return Err(ClipboardError::UnsupportedFormat(data.format()));
        };
        self.set_text(kind, text.as_str().to_owned(), options.exclude_from_history)
    }

    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
        self.set_text(kind, String::new(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopClipboard;
    use zgui_platform::{Clipboard, ClipboardData, ClipboardFormat, ClipboardKind};

    #[test]
    fn a_representation_this_boundary_cannot_carry_is_refused_rather_than_emptied() {
        // An empty answer and a refusal mean opposite things to a caller deciding whether to offer
        // a paste command: one greys it out, the other hides it.
        let clipboard = DesktopClipboard::new();
        for format in [
            ClipboardFormat::Html,
            ClipboardFormat::FileList,
            ClipboardFormat::Image,
        ] {
            let refusal = clipboard
                .read_blocking(ClipboardKind::Standard, format)
                .expect_err("this boundary carries only text");
            assert!(
                refusal.is_permanent(),
                "{format:?} was refused as if it might work later"
            );
        }
    }

    #[test]
    fn writing_something_that_is_not_text_is_refused_without_touching_the_desktop() {
        let clipboard = DesktopClipboard::new();
        let refusal = clipboard
            .write(
                ClipboardKind::Standard,
                ClipboardData::FileList(Vec::new()),
                zgui_platform::ClipboardWriteOptions::default(),
            )
            .expect_err("this boundary carries only text");
        assert!(refusal.is_permanent());
    }

    #[test]
    fn every_read_started_through_the_loop_gets_its_own_identity() {
        // Two paste requests can be in flight at once — a slow selection owner and an impatient
        // user produce it — and an answer with no identity would be applied to the wrong one.
        let clipboard = DesktopClipboard::new();
        let first = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Html);
        let second = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Html);
        assert_ne!(first, second);
    }
}
