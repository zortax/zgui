//! What this desktop means by a scroll, as far as a windowing backend can know it.
//!
//! Three answers, and each one differs by operating system rather than by machine. They are stated
//! here, once, so that nothing above the platform seam has to ask which desktop it is on — and so
//! that the day one of them turns out to be wrong there is a single place to correct it.

use zgui_platform::scroll::{ScrollDirection, ScrollSettings, WheelMotion};

/// What a scroll from this desktop's devices means.
///
/// The direction preference is [`ScrollDirection::AsReported`] on every target, and that is not a
/// shortcut. Natural scrolling lives in the input stack on all three families — libinput on Linux,
/// the window server on macOS, the mouse driver and Precision Touchpad stack on Windows — so a
/// delta has already been pointed the way the person asked for by the time any window sees it.
/// Applying it again here would silently override a desktop setting for every program built with
/// this framework.
///
/// ```
/// use zgui_platform::scroll::ScrollDirection;
///
/// let settings = zgui_platform_winit::desktop_scroll_settings();
/// assert_eq!(settings.direction, ScrollDirection::AsReported);
/// assert!(settings.lines_per_notch >= 1.0);
/// ```
pub fn desktop_scroll_settings() -> ScrollSettings {
    ScrollSettings::desktop()
        .with_lines_per_notch(LINES_PER_NOTCH)
        .with_direction(ScrollDirection::AsReported)
        .with_wheel(WHEEL)
}

/// How many lines of text one detent of a notched wheel asks for here.
///
/// Three on the desktops whose applications move three, which was measured rather than assumed: one
/// detent from a virtual mouse into a terminal on an ordinary Wayland session moved its content by
/// exactly three lines, twice in a row, and the same count is what GTK, Qt and every browser on
/// such a session use. macOS has no line unit for the wheel at all — its scroll events are pixels
/// from the window server, already smoothed — so the count is only ever consulted there for a mouse
/// that reports detents, where three is again what its applications do.
#[cfg(not(target_os = "macos"))]
const LINES_PER_NOTCH: f32 = 3.0;

/// How many lines of text one detent of a notched wheel asks for here.
///
/// See the value used on every other target: three is what the applications on this desktop move,
/// and a wheel that travels a third of that feels broken while every number in it is consistent.
#[cfg(target_os = "macos")]
const LINES_PER_NOTCH: f32 = 3.0;

/// Whether a detent arrives whole or already spread over time.
///
/// X11, Wayland and Win32 all deliver one detent as one axis event: nothing has animated it, and
/// something above this seam has to carry the content to its new place over several frames or the
/// document jumps a hundred pixels per click.
#[cfg(not(target_os = "macos"))]
const WHEEL: WheelMotion = WheelMotion::Discrete;

/// Whether a detent arrives whole or already spread over time.
///
/// The macOS window server animates scrolling itself, including the momentum after the fingers have
/// left the trackpad, and delivers the result as a stream of small pixel deltas. Animating that
/// again would be animating each of its thirty little deltas separately, which crawls.
#[cfg(target_os = "macos")]
const WHEEL: WheelMotion = WheelMotion::Continuous;

#[cfg(test)]
mod tests {
    use zgui_platform::scroll::ScrollDirection;

    use super::desktop_scroll_settings;

    #[test]
    fn the_direction_preference_is_never_applied_twice() {
        assert_eq!(
            desktop_scroll_settings().direction,
            ScrollDirection::AsReported,
            "natural scrolling is a setting of the input stack; flipping it here overrides it"
        );
    }

    #[test]
    fn one_detent_asks_for_as_many_lines_as_this_desktops_applications_move() {
        assert_eq!(desktop_scroll_settings().lines_for(1.0), 3.0);
    }
}
