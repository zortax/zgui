//! The element vocabulary, and the marker type behind each name.
//!
//! Sixteen names, chosen for what an interface is made of rather than for what a document is made
//! of. There is no `div`, because a container that means nothing is what `box` says out loud; there
//! is no `span`, because the two things a `span` does — a run of inline text and a labelled inline
//! box — are `text` and `label`; and there is no `input`, because a text field, a checkbox and a
//! slider have nothing in common but a specification's history.
//!
//! Each name is a marker type and a function of the same name that starts a builder. The marker is
//! what carries the typed attributes: `src` exists on [`image()`] and not on [`row()`], and the
//! difference is a compile error rather than an attribute nothing reads.
//!
//! # What the names mean
//!
//! | Name | What it is | Laid out as |
//! |---|---|---|
//! | `box` ([`Box_`]) | a container that means nothing in particular | a block |
//! | [`row()`] | children in a line, left to right | a flex row |
//! | [`column()`] | children in a line, top to bottom | a flex column |
//! | [`stack()`] | children over one another | a flex column, positioned |
//! | [`text()`] | a run of text | inline |
//! | [`label()`] | text naming something else | inline |
//! | [`image()`] | a picture | an inline block |
//! | [`vector()`] | shapes, drawn from paths | an inline block |
//! | [`scroll()`] | content larger than the space for it | a block that scrolls |
//! | [`canvas()`] | shapes the application draws and mutates | an inline block, 300×150 unstyled |
//! | [`editor()`] | text the user changes | a block |
//! | [`field()`] | one value the user enters | a block |
//! | [`control()`] | something the user operates | a block |
//! | [`surface()`] | a raised region: a card, a sheet, a menu | a block |
//! | [`spacer()`] | the space between two things | a block that grows |
//! | [`overlay_root()`] | where portalled content goes | a fixed, window-sized block |
//!
//! The layout column is what the framework's own style sheet says. It is CSS, so it is overridable
//! like any other declaration: `column { flex-direction: row }` works and means what it says.

mod markers;

pub use crate::tag::markers::{
    Box_, Canvas, Column, Control, Editor, Field, Image, Label, OverlayRoot, Row, Scroll, Spacer,
    Stack, Surface, Text, Vector, r#box, canvas, column, control, editor, field, image, label,
    overlay_root, row, scroll, spacer, stack, surface, text, vector,
};

use zgui_interned::ElementName;

/// One element name.
///
/// A marker type implements this to say what it is called and which typed attributes it takes.
/// Implementing it outside this crate is how a document language of one's own — a browser's HTML,
/// a diagram format — joins the same builder machinery: `<html::div/>` in a view is a call to a
/// function returning [`Element`](crate::Element) over that language's own marker.
pub trait Tag: 'static {
    /// What the element is called, which is what selectors match and what the framework's own
    /// style sheet gives its layout defaults to.
    fn name() -> ElementName;
}
