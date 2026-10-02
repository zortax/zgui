//! The `NSWindow` behind a winit window.

use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The AppKit window `window` draws into.
pub(crate) fn ns_window(window: &winit::window::Window) -> Option<Retained<NSWindow>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit hands out its own content view, which lives as long as the window, and the
    // window is borrowed for this call. Retaining it keeps it alive past the borrow.
    let view: Retained<NSView> = unsafe { Retained::retain(handle.ns_view.as_ptr().cast()) }?;
    view.window()
}
