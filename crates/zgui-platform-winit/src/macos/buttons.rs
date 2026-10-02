//! Where the close, minimize and zoom buttons sit over a window with no title bar.

use objc2_app_kit::{NSWindowButton, NSWindowStyleMask};
use objc2_foundation::NSPoint;
use zgui_platform::TitleButtons;

use crate::macos::window::ns_window;

/// Moves the window buttons of `window` into the band `buttons` names.
///
/// AppKit lays the title bar out again on a resize, on a change of key window and on leaving full
/// screen, so the caller places the buttons again after each.
pub(crate) fn place(window: &winit::window::Window, buttons: TitleButtons) {
    let Some(ns_window) = ns_window(window) else {
        return;
    };
    // A full-screen window shows its buttons in the menu-bar strip, where AppKit owns them.
    if ns_window
        .styleMask()
        .contains(NSWindowStyleMask::FullScreen)
    {
        return;
    }
    let close = ns_window.standardWindowButton(NSWindowButton::CloseButton);
    let minimize = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton);
    let zoom = ns_window.standardWindowButton(NSWindowButton::ZoomButton);
    let (Some(close), Some(minimize), Some(zoom)) = (close, minimize, zoom) else {
        return;
    };

    let band = f64::from(buttons.band.0);
    // SAFETY: the buttons belong to the window, which is alive for this call, and so do the views
    // that hold them.
    let container = unsafe { close.superview().and_then(|bar| bar.superview()) };
    if let Some(container) = container {
        let mut frame = container.frame();
        frame.size.height = band;
        frame.origin.y = ns_window.frame().size.height - band;
        container.setFrame(frame);
    }

    let spacing = minimize.frame().origin.x - close.frame().origin.x;
    for (index, button) in [close, minimize, zoom].into_iter().enumerate() {
        let height = button.frame().size.height;
        let x = f64::from(buttons.left.0) + index as f64 * spacing;
        button.setFrameOrigin(NSPoint::new(x, ((band - height) / 2.0).round()));
    }
}
