//! What can be asked of an editor, and what comes back.

use crate::select::{Granularity, Motion, Selection};
use crate::text::Splice;

/// One thing to do to the text.
///
/// Everything an editor does is one of these, including what a key press means: the mapping from
/// keys to commands is a separate, replaceable step ([`keys`](crate::editor::keys)), so an
/// application that rebinds its keyboard changes that mapping and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Put this text in, replacing whatever is selected.
    Insert(String),
    /// Remove the selection, or one unit before the caret when nothing is selected.
    DeleteBackwards(Granularity),
    /// Remove the selection, or one unit after the caret when nothing is selected.
    DeleteForwards(Granularity),
    /// Move the caret.
    Move(Motion),
    /// Put the selection exactly here.
    Select(Selection),
    /// Select everything.
    SelectAll,
    /// Take back the last change.
    Undo,
    /// Put back the last change that was taken back.
    Redo,
    /// Copy the selection.
    Copy,
    /// Copy the selection and remove it.
    Cut,
    /// Replace the selection with this text, as one change.
    Paste(String),
}

/// What an editor did about something it was asked.
///
/// A command that changed nothing reports nothing, which is what tells a caller whether to reshape
/// a paragraph, redraw a caret or leave the frame alone — and a key the editor did not take is
/// reported as not handled, so the key reaches whatever else is listening for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    /// Whether the editor took the event.
    pub handled: bool,
    /// Which paragraphs changed, when the text did.
    pub splice: Option<Splice>,
    /// The selection afterwards, when it moved.
    pub selection: Option<Selection>,
    /// Text the editor asks to be placed on the clipboard.
    pub clipboard: Option<String>,
}

impl Response {
    /// Nothing happened and the event is somebody else's.
    pub fn ignored() -> Self {
        Self::default()
    }

    /// The event was taken and nothing else came of it.
    pub fn handled() -> Self {
        Self {
            handled: true,
            ..Self::default()
        }
    }

    /// Whether the text itself changed.
    pub fn changed_text(&self) -> bool {
        self.splice.is_some()
    }
}
