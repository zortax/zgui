//! How the colour stripes of the display are arranged, where the desktop says.
//!
//! Windows states how it draws its own text: with ClearType or without it, and in which stripe
//! order. That is a setting of the user for the whole desktop, so it is the answer for every
//! window. macOS draws text with one coverage for each pixel. The other desktops this backend
//! runs on state nothing through winit.

use zgui_platform::SubpixelLayout;

/// The layout the desktop states, read from the system on every call.
///
/// A window asks once a frame, so a change of the setting shows on the next frame the window
/// draws.
#[cfg(target_os = "windows")]
pub(crate) fn layout() -> SubpixelLayout {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FE_FONTSMOOTHINGCLEARTYPE, FE_FONTSMOOTHINGORIENTATIONBGR, FE_FONTSMOOTHINGORIENTATIONRGB,
        SPI_GETFONTSMOOTHING, SPI_GETFONTSMOOTHINGORIENTATION, SPI_GETFONTSMOOTHINGTYPE,
    };

    // Font smoothing off, and smoothing of the standard type, are both one coverage for each
    // pixel. A setting the system refuses to state is unknown.
    let Some(smoothing) = parameter(SPI_GETFONTSMOOTHING) else {
        return SubpixelLayout::Unknown;
    };
    if smoothing == 0 {
        return SubpixelLayout::None;
    }
    match parameter(SPI_GETFONTSMOOTHINGTYPE) {
        Some(FE_FONTSMOOTHINGCLEARTYPE) => {}
        Some(_) => return SubpixelLayout::None,
        None => return SubpixelLayout::Unknown,
    }
    match parameter(SPI_GETFONTSMOOTHINGORIENTATION) {
        Some(FE_FONTSMOOTHINGORIENTATIONRGB) => SubpixelLayout::HorizontalRgb,
        Some(FE_FONTSMOOTHINGORIENTATIONBGR) => SubpixelLayout::HorizontalBgr,
        _ => SubpixelLayout::Unknown,
    }
}

/// One system parameter that is a 32-bit value, or `None` when the system refuses the query.
#[cfg(target_os = "windows")]
fn parameter(action: u32) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;

    let mut value: u32 = 0;
    // SAFETY: each action this module asks for writes one 32-bit value through `pvParam`: a
    // `BOOL` for the smoothing flag and a `UINT` for the type and the orientation. `value` is a
    // live, writable `u32` for the length of the call.
    let answered =
        unsafe { SystemParametersInfoW(action, 0, core::ptr::from_mut(&mut value).cast(), 0) };
    (answered != 0).then_some(value)
}

/// The layout the desktop states: none, because macOS draws text with one coverage for each
/// pixel.
#[cfg(target_os = "macos")]
pub(crate) fn layout() -> SubpixelLayout {
    SubpixelLayout::None
}

/// The layout the desktop states: unknown, because winit states none on this desktop.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn layout() -> SubpixelLayout {
    SubpixelLayout::Unknown
}
