//! Telling the platform that text is being typed, and where.
//!
//! Two things have to be right for an input method to work, and neither of them is about the text.
//! The window has to be told that text input is wanted at all — until it is, no composition is
//! ever started and a Japanese keyboard produces nothing — and it has to be told where the caret
//! is, because that is where it puts the candidate window. Both are per focus change and per caret
//! move, which is what [`Ime`] keeps track of so the surface is not told the same thing twice a
//! frame.

pub mod area;
pub mod preedit;

pub use crate::ime::area::caret_area;
pub use crate::ime::preedit::Preedit;

use zgui_dom::NodeKey;
use zgui_platform::TextInput;

/// What the window has been told about text input, and what it is owed.
///
/// Held by whatever owns the surface. Every method returns what the platform must be told, or
/// nothing when it already knows — a surface told the caret is where it already is wakes the input
/// method for nothing, once per frame, for as long as the field has focus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ime {
    /// The element text input is enabled for.
    target: Option<NodeKey>,
    /// What the surface was last told.
    told: Option<TextInput>,
    /// Whether an input method has provisional text on the screen.
    preedit: Preedit,
}

/// What the surface must be told about text input.
#[derive(Clone, Debug, PartialEq)]
pub enum Told {
    /// Text is being typed here, with the caret in this state.
    Enabled(TextInput),
    /// No text is being typed; the input method is dismissed.
    Disabled,
}

impl Ime {
    /// Nothing focused and nothing composed.
    pub fn new() -> Self {
        Self::default()
    }

    /// The element text input is enabled for.
    pub fn target(&self) -> Option<NodeKey> {
        self.target
    }

    /// Whether provisional text is on the screen.
    pub fn is_composing(&self) -> bool {
        self.preedit.is_active()
    }

    /// The composition, as the platform reports it.
    pub fn preedit(&self) -> &Preedit {
        &self.preedit
    }

    /// Follows one step of a composition.
    pub fn observe(&mut self, event: &zgui_vocab::ImeEvent) {
        self.preedit.observe(event);
    }

    /// Records that focus moved to an editable element with its caret at `area`.
    ///
    /// Returns what the surface must be told. Focus leaving an editable element abandons any
    /// composition: the provisional text belonged to the field that no longer has focus, and an
    /// input method left enabled would commit it into whatever gained it.
    pub fn focused(&mut self, node: Option<NodeKey>, area: Option<TextInput>) -> Option<Told> {
        let wanted = node.zip(area);
        match wanted {
            Some((node, area)) => {
                if self.target == Some(node) && self.told.as_ref() == Some(&area) {
                    return None;
                }
                self.target = Some(node);
                self.told = Some(area);
                Some(Told::Enabled(area))
            }
            None => {
                if self.target.is_none() && self.told.is_none() {
                    return None;
                }
                self.target = None;
                self.told = None;
                self.preedit.clear();
                Some(Told::Disabled)
            }
        }
    }

    /// Records that the caret moved inside the element that already has text input enabled.
    pub fn caret_moved(&mut self, area: TextInput) -> Option<Told> {
        self.target?;
        if self.told.as_ref() == Some(&area) {
            return None;
        }
        self.told = Some(area);
        Some(Told::Enabled(area))
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Point, Size};
    use zgui_platform::TextInput;
    use zgui_vocab::ImeEvent;

    use super::{Ime, Told};

    /// A caret area at `x`.
    fn area(x: f32) -> TextInput {
        TextInput::new(
            Point::new(CssPx(x), CssPx(20.0)),
            Size::new(CssPx(1.0), CssPx(16.0)),
        )
    }

    /// A node handle to aim at.
    fn node() -> zgui_dom::NodeKey {
        let document = zgui_dom::Document::new();
        document.store().key_of(document.document_index())
    }

    #[test]
    fn focusing_a_field_turns_text_input_on_and_says_where_the_caret_is() {
        let mut ime = Ime::new();
        assert_eq!(
            ime.focused(Some(node()), Some(area(10.0))),
            Some(Told::Enabled(area(10.0)))
        );
    }

    #[test]
    fn being_told_the_same_thing_twice_tells_the_platform_once() {
        // A surface told the caret is where it already is wakes the input method every frame for
        // as long as the field has focus, which is the whole of what this exists to prevent.
        let mut ime = Ime::new();
        let field = node();
        ime.focused(Some(field), Some(area(10.0)));
        assert_eq!(ime.focused(Some(field), Some(area(10.0))), None);
        assert_eq!(ime.caret_moved(area(10.0)), None);
        assert_eq!(
            ime.caret_moved(area(24.0)),
            Some(Told::Enabled(area(24.0))),
            "a caret that really moved is reported"
        );
    }

    #[test]
    fn losing_focus_turns_text_input_off_and_abandons_the_composition() {
        let mut ime = Ime::new();
        ime.focused(Some(node()), Some(area(10.0)));
        ime.observe(&ImeEvent::Preedit {
            text: "に".into(),
            cursor: None,
        });
        assert!(ime.is_composing());

        assert_eq!(ime.focused(None, None), Some(Told::Disabled));
        assert!(!ime.is_composing());
        assert_eq!(ime.target(), None);
        assert_eq!(ime.focused(None, None), None, "and stays off, silently");
    }

    #[test]
    fn a_caret_reported_before_anything_is_focused_tells_the_platform_nothing() {
        let mut ime = Ime::new();
        assert_eq!(ime.caret_moved(area(4.0)), None);
    }
}
