//! What this desktop means by a scroll.

use zgui_platform::scroll::{ScrollDirection, ScrollSettings, WheelMotion};

/// What a scroll from this desktop's devices means.
///
/// The direction preference is [`ScrollDirection::AsReported`], and that is not a shortcut.
/// Natural scrolling lives in the input stack here: libinput applies it before the compositor sees
/// the axis event, so a delta has already been pointed the way the person asked for by the time any
/// window sees it. Applying it again would silently override a desktop setting for every program
/// built with this framework.
///
/// ```
/// use zgui_platform::scroll::{ScrollDirection, WheelMotion};
///
/// let settings = zgui_platform_wayland::desktop_scroll_settings();
/// assert_eq!(settings.direction, ScrollDirection::AsReported);
/// assert_eq!(settings.wheel, WheelMotion::Discrete);
/// assert!(settings.lines_per_notch >= 1.0);
/// ```
pub fn desktop_scroll_settings() -> ScrollSettings {
    ScrollSettings::desktop()
        .with_lines_per_notch(LINES_PER_NOTCH)
        .with_direction(ScrollDirection::AsReported)
        .with_wheel(WheelMotion::Discrete)
}

/// How many lines of text one detent of a notched wheel asks for here.
///
/// Three, measured rather than assumed: one detent from a virtual mouse into a terminal on an
/// ordinary Wayland session moved its content by exactly three lines, and the same count is what
/// GTK, Qt and every browser on such a session use.
const LINES_PER_NOTCH: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::desktop_scroll_settings;
    use zgui_platform::scroll::{ScrollDirection, WheelMotion};

    #[test]
    fn a_scroll_arrives_pointing_the_way_the_person_asked_for() {
        // Flipping it here would override a desktop setting on every machine.
        assert_eq!(
            desktop_scroll_settings().direction,
            ScrollDirection::AsReported
        );
    }

    #[test]
    fn a_detent_arrives_whole_and_something_above_has_to_carry_it() {
        // One detent is one axis event here: nothing has animated it, so a document that applied
        // it directly would jump a hundred pixels per click.
        assert_eq!(desktop_scroll_settings().wheel, WheelMotion::Discrete);
    }

    #[test]
    fn a_detent_is_worth_the_lines_this_desktops_applications_move() {
        assert_eq!(desktop_scroll_settings().lines_per_notch, 3.0);
    }
}
