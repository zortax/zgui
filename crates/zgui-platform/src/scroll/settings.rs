//! Everything the desktop decides about scrolling, as one value.

use crate::scroll::direction::ScrollDirection;
use crate::scroll::elastic::Elastic;
use crate::scroll::motion::WheelMotion;

/// What a scroll from this desktop's devices means.
///
/// A backend fills this in from what it knows about the machine it is on; nothing above the
/// platform seam names a desktop, and nothing below it decides how far a document should travel.
///
/// ```
/// use zgui_platform::scroll::{ScrollDirection, ScrollSettings, WheelMotion};
///
/// // The answer on an ordinary Linux or Windows desktop.
/// let settings = ScrollSettings::desktop();
/// assert_eq!(settings.lines_per_notch, 3.0);
/// assert_eq!(settings.direction, ScrollDirection::AsReported);
/// assert!(settings.wheel.framework_animates());
///
/// // A precision surface animates its own scrolling, so nothing above may animate it again.
/// let precise = ScrollSettings::desktop().with_wheel(WheelMotion::Continuous);
/// assert!(!precise.wheel.framework_animates());
///
/// // And a notched wheel gets an edge that stops rather than one that springs.
/// assert!(!settings.elastic.admits_a_detent());
/// assert!(settings.elastic.admits_a_gesture());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScrollSettings {
    /// How many lines of text one detent of a notched wheel asks for.
    ///
    /// Lines rather than pixels, because a line is a property of the thing being scrolled and a
    /// pixel count is not: the same detent has to travel further over a list of headings than over
    /// a list of table rows, and a pixel constant here makes the wheel feel wrong on one of them
    /// whichever number is chosen. What this contributes is the *count*, which is the part the
    /// desktop decides.
    pub lines_per_notch: f32,
    /// Whether the person's scroll-direction preference has already been applied.
    pub direction: ScrollDirection,
    /// Whether a detent arrives whole or as a stream the platform is already animating.
    pub wheel: WheelMotion,
    /// Which inputs may drag a container past its own end.
    pub elastic: Elastic,
}

impl ScrollSettings {
    /// What an ordinary desktop means by a scroll.
    ///
    /// Three lines per detent, the direction already settled by the input stack, and a detent that
    /// arrives whole. Three is not a taste: it is what the applications already on such a desktop
    /// do, and a framework whose wheel travels a third as far as everything else on the screen is
    /// a framework that feels broken while every one of its own numbers is self-consistent.
    pub const fn desktop() -> Self {
        Self {
            lines_per_notch: 3.0,
            direction: ScrollDirection::AsReported,
            wheel: WheelMotion::Discrete,
            elastic: Elastic::Kinetic,
        }
    }

    /// The same with a different count of lines per detent.
    pub const fn with_lines_per_notch(mut self, lines: f32) -> Self {
        self.lines_per_notch = lines;
        self
    }

    /// The same with a different answer about the direction preference.
    pub const fn with_direction(mut self, direction: ScrollDirection) -> Self {
        self.direction = direction;
        self
    }

    /// The same with a different answer about who animates a detent.
    pub const fn with_wheel(mut self, wheel: WheelMotion) -> Self {
        self.wheel = wheel;
        self
    }

    /// The same with a different answer about which inputs may stretch an edge.
    pub const fn with_elastic(mut self, elastic: Elastic) -> Self {
        self.elastic = elastic;
        self
    }

    /// How many lines `notches` detents ask for, never fewer than the notches themselves.
    ///
    /// A backend that reported a count of zero — or a negative one — would otherwise produce a
    /// wheel that does nothing at all, or one that goes backwards for a reason nobody could find
    /// by reading the scrolling code.
    pub fn lines_for(self, notches: f32) -> f32 {
        notches * self.lines_per_notch.max(1.0)
    }
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self::desktop()
    }
}

#[cfg(test)]
mod tests {
    use crate::scroll::{ScrollDirection, WheelMotion};

    use super::ScrollSettings;

    #[test]
    fn a_backend_that_says_nothing_gets_what_an_ordinary_desktop_does() {
        assert_eq!(ScrollSettings::default(), ScrollSettings::desktop());
    }

    #[test]
    fn three_detents_ask_for_three_times_what_one_does() {
        let settings = ScrollSettings::desktop();
        assert_eq!(settings.lines_for(1.0), 3.0);
        assert_eq!(settings.lines_for(3.0), 9.0);
        assert_eq!(settings.lines_for(-1.0), -3.0);
    }

    #[test]
    fn a_nonsense_count_still_moves_the_document_by_the_detents_themselves() {
        // A backend that fills this in from a desktop setting it misread must not produce a wheel
        // that silently does nothing.
        assert_eq!(
            ScrollSettings::desktop()
                .with_lines_per_notch(0.0)
                .lines_for(2.0),
            2.0
        );
        assert_eq!(
            ScrollSettings::desktop()
                .with_lines_per_notch(-5.0)
                .lines_for(2.0),
            2.0,
            "a negative count must not reverse the wheel behind the direction setting's back"
        );
    }

    #[test]
    fn each_answer_can_be_replaced_without_disturbing_the_others() {
        let settings = ScrollSettings::desktop()
            .with_direction(ScrollDirection::Inverted)
            .with_wheel(WheelMotion::Continuous);
        assert_eq!(settings.lines_per_notch, 3.0);
        assert_eq!(settings.direction, ScrollDirection::Inverted);
        assert_eq!(settings.wheel, WheelMotion::Continuous);
    }
}
