//! Telling the platform where text is being typed, and what kind.

use zgui_geom::{Css, CssPx, Point, Size};

/// What kind of text a field expects, so the platform can offer the right keyboard.
///
/// On a desktop with a hardware keyboard this changes little. On a touch platform it changes
/// everything: a field that asks for a number and gets a full keyboard is a field that is painful
/// to use, and the platform has no other way to know.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextInputPurpose {
    /// Ordinary text.
    #[default]
    Normal,
    /// A secret, which must not be remembered, suggested or shown.
    Password,
    /// A personal identification number, which is a secret made of digits.
    Pin,
    /// A number.
    Number,
    /// A telephone number.
    Phone,
    /// A web address.
    Url,
    /// An electronic mail address.
    Email,
    /// A term to search for.
    Search,
}

impl TextInputPurpose {
    /// Whether what is typed here must never be recorded or suggested.
    pub const fn is_secret(self) -> bool {
        matches!(self, Self::Password | Self::Pin)
    }
}

/// Everything the platform needs to know about the text being typed right now.
///
/// The three parts travel together because they are set together. A platform that is told the
/// caret moved but not that the field is still active, or told a purpose before it has an area to
/// place a candidate window in, shows its input method in the wrong place — which for a language
/// that composes its characters means typing over the top of what is being typed.
///
/// The area is where the *caret* is, not where the field is, so a candidate list appears beside
/// the insertion point rather than beside the control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInput {
    /// Where the caret sits, in CSS pixels from the surface's top-left corner.
    pub caret_origin: Point<CssPx, Css>,
    /// How large the caret is, so a candidate window can avoid covering it.
    pub caret_size: Size<CssPx, Css>,
    /// What kind of text is expected.
    pub purpose: TextInputPurpose,
}

impl TextInput {
    /// Ordinary text with a caret of the given size at the given place.
    pub const fn new(caret_origin: Point<CssPx, Css>, caret_size: Size<CssPx, Css>) -> Self {
        Self {
            caret_origin,
            caret_size,
            purpose: TextInputPurpose::Normal,
        }
    }

    /// The same state with a different purpose.
    pub const fn with_purpose(mut self, purpose: TextInputPurpose) -> Self {
        self.purpose = purpose;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{TextInput, TextInputPurpose};
    use zgui_geom::{CssPx, Point, Size};

    #[test]
    fn only_the_secret_purposes_are_secret() {
        assert!(TextInputPurpose::Password.is_secret());
        assert!(TextInputPurpose::Pin.is_secret());
        assert!(!TextInputPurpose::Email.is_secret());
        assert!(!TextInputPurpose::default().is_secret());
    }

    #[test]
    fn the_state_carries_the_caret_rather_than_the_field() {
        let state = TextInput::new(
            Point::new(CssPx(120.0), CssPx(48.0)),
            Size::new(CssPx(1.0), CssPx(16.0)),
        )
        .with_purpose(TextInputPurpose::Search);
        assert_eq!(state.caret_size.width, CssPx(1.0));
        assert_eq!(state.purpose, TextInputPurpose::Search);
    }
}
