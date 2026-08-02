//! The attributes that belong to one name rather than to all of them.
//!
//! `src` is a picture's, `paths` are a drawing's, and a value is something the user typed. Each is
//! reachable only on the names it means something for, so `row().src("…")` does not compile — and
//! that is the whole reason the vocabulary is typed rather than a string and a bag of attributes.

use zgui_interned::AttrName;
use zgui_view::{IntoReactiveValue, PropKey, PropValue, UiState};

use crate::element::Element;
use crate::tag::{Control, Editor, Field, Image, Tag};

/// An element whose content comes from somewhere named.
///
/// ```
/// zgui_elements::image().src("picture.png");
/// ```
///
/// A name that is not a picture has no source, and saying it has one does not compile:
///
/// ```compile_fail
/// zgui_elements::row().src("picture.png");
/// ```
pub trait Sourced: Tag {}
impl Sourced for Image {}

/// An element the user puts a value into.
///
/// ```
/// zgui_elements::field().placeholder("Search");
/// ```
///
/// ```compile_fail
/// zgui_elements::row().placeholder("Search");
/// ```
pub trait Valued: Tag {}
impl Valued for Field {}
impl Valued for Editor {}

/// An element the user operates, and can therefore be stopped from operating.
///
/// ```
/// zgui_elements::control().disabled(true);
/// ```
///
/// A run of text is not something anyone operates, so there is nothing to disable:
///
/// ```compile_fail
/// zgui_elements::text().disabled(true);
/// ```
pub trait Operable: Tag {}
impl Operable for Control {}
impl Operable for Field {}
impl Operable for Editor {}

impl<T: Sourced> Element<T> {
    /// Where the content comes from.
    #[must_use]
    pub fn src<M>(self, source: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.attribute(AttrName::new("src"), source)
    }

    /// What the content shows, for anything reading the interface rather than looking at it.
    ///
    /// An empty description says the picture is decorative and carries nothing a reader needs,
    /// which is different from saying nothing at all.
    #[must_use]
    pub fn alt<M>(self, description: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.attribute(AttrName::new("alt"), description)
    }
}

impl<T: Valued> Element<T> {
    /// The value the element currently holds.
    ///
    /// A property rather than an attribute, deliberately: a value changes on every keystroke, and
    /// were it an attribute every keystroke would invalidate selector matching for the subtree.
    #[must_use]
    pub fn value<M>(self, value: impl IntoReactiveValue<PropValue, M>) -> Self {
        self.property(PropKey::new("value"), value)
    }

    /// What to show while the element is empty.
    #[must_use]
    pub fn placeholder<M>(self, text: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.attribute(AttrName::new("placeholder"), text)
    }
}

impl<T: Operable> Element<T> {
    /// Whether the user is stopped from operating this element.
    ///
    /// Sets the interaction state `:disabled` matches, which is also what removes the element from
    /// the focus order and from what a pointer can reach.
    #[must_use]
    pub fn disabled<M>(self, disabled: impl IntoReactiveValue<bool, M>) -> Self {
        self.state(UiState::DISABLED, disabled)
    }
}
