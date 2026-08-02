//! Forcing a paragraph's base direction, and the one control that actually does it.

use zgui_text_style::Direction;

/// The character that forces a right-to-left base level: U+200F, `RIGHT-TO-LEFT MARK`.
///
/// ```
/// use zgui_text_parley::{Controls, RIGHT_TO_LEFT_MARK};
/// use zgui_text_style::Direction;
///
/// let prefix = Controls::Mark.prefix(Direction::RightToLeft);
/// assert_eq!(prefix.chars().next(), Some(RIGHT_TO_LEFT_MARK));
/// ```
pub const RIGHT_TO_LEFT_MARK: char = '\u{200f}';

/// The character that forces a left-to-right base level: U+200E, `LEFT-TO-RIGHT MARK`.
///
/// ```
/// use zgui_text_parley::{Controls, LEFT_TO_RIGHT_MARK};
/// use zgui_text_style::Direction;
///
/// let prefix = Controls::Mark.prefix(Direction::LeftToRight);
/// assert_eq!(prefix.chars().next(), Some(LEFT_TO_RIGHT_MARK));
/// ```
pub const LEFT_TO_RIGHT_MARK: char = '\u{200e}';

/// Which mechanism a paragraph's base direction is forced with.
///
/// # Why this is a choice a caller makes and not a constant
///
/// A text engine detects the base direction from the text it is given and offers no override, so
/// the only lever is what goes into that text. There are two things a caller can want, and they
/// are not variations on one behaviour:
///
/// * the paragraph's direction comes from the style, and the engine has to be *told* — that is
///   [`Controls::Mark`], and it is what a document lays text out with;
/// * the generated string already carries whatever directional controls the caller intended, and
///   adding another would change the answer — that is [`Controls::Verbatim`], which is what a
///   caller replaying a stored string, or implementing `unicode-bidi: plaintext`, needs.
///
/// # What is deliberately not offered
///
/// Wrapping the paragraph in an isolate pair. The bidirectional algorithm's paragraph-level rule
/// skips every character between an isolate initiator and its matching pop, so an isolate around
/// the *whole* paragraph hides every strong character from the detection and the base level falls
/// through to left-to-right. The content still reorders correctly, which is what makes the failure
/// hard to see: the paragraph reads right and then aligns to the wrong edge. Isolates remain the
/// right encoding for an *inner* span, where the rule they trip is the point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Controls {
    /// Prefix a directional mark, which sets the base level and draws nothing.
    ///
    /// The mark is a formatting character of common script, so it contributes to no shape and
    /// produces no glyph, while remaining a strong character for base-level detection.
    #[default]
    Mark,
    /// Add nothing; the string already says what it means.
    Verbatim,
}

impl Controls {
    /// The text prefixed to a paragraph laid out in `direction`.
    ///
    /// ```
    /// use zgui_text_parley::Controls;
    /// use zgui_text_style::Direction;
    ///
    /// assert_eq!(Controls::Mark.prefix(Direction::RightToLeft), "\u{200f}");
    /// assert_eq!(Controls::Verbatim.prefix(Direction::RightToLeft), "");
    /// ```
    pub fn prefix(self, direction: Direction) -> &'static str {
        match (self, direction) {
            (Self::Verbatim, _) => "",
            (Self::Mark, Direction::LeftToRight) => "\u{200e}",
            (Self::Mark, Direction::RightToLeft) => "\u{200f}",
        }
    }
}
