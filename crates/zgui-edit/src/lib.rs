//! Editing text: the caret, the selection, undo, composition, and where a click lands.
//!
//! Everything an editable field or an editable document does to its text is here, and nothing
//! about how that text is drawn. The model is asked what to do and answers what changed; the
//! layout is consulted only where it has to be, which is the two questions that are genuinely
//! about the shaped glyphs — where a point landed, and where the caret goes.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`text`] | the buffer, split into the paragraphs a shaper works in |
//! | [`select`] | the caret, the selection, and how far one movement goes |
//! | [`history`] | undo and redo, with consecutive typing folded into one step |
//! | [`ime`] | the provisional text an input method is still deciding on |
//! | [`hit`] | a point to an offset, and an offset to a caret, over the shaper's own clusters |
//! | [`editor`] | all of the above, driven by commands and by key and input-method events |
//! | [`document`] | the text nodes the paragraphs are written into |
//!
//! # The three decisions everything else follows from
//!
//! **The buffer is a list of paragraphs.** A shaper has no incremental mode — one inserted
//! character re-shapes a whole shaped result — so the unit of change has to be the unit of
//! shaping. Every replacement therefore reports which paragraphs it touched, one text node holds
//! one paragraph, and a keystroke re-shapes one of them however long the document is.
//!
//! **A composition is authoritative while it runs.** The platform keeps delivering the keys an
//! input method did not consume, on X11 and on Wayland alike. An editor that acts on them moves
//! the caret out from under the provisional text, and the commit that follows lands where the
//! caret went rather than where the text being composed is. So [`Editor::key`] refuses keys while
//! [`Editor::is_composing`] holds, and reports them as unhandled so that everything else still
//! hears them.
//!
//! **A cluster is what can be selected, and only the shaper knows what they are.** A ligature is
//! one cluster of several characters and a caret may not be placed inside it; a bidirectional line
//! draws its bytes in an order the string does not have. So hit testing is done over the clusters
//! the shaper reports, and one offset at a direction boundary has two carets, told apart by
//! [`Affinity`].
//!
//! ```
//! use zgui_edit::Editor;
//! use zgui_edit::editor::Command;
//! use zgui_edit::select::Selection;
//!
//! let mut editor = Editor::new("one\ntwo");
//! editor.set_selection(Selection::caret(7));
//! let response = editor.apply(Command::Insert("!".to_owned()));
//!
//! let splice = response.splice.expect("the text changed");
//! assert_eq!(splice.removed, 1..2, "the second paragraph, and nothing else");
//! assert_eq!(editor.text(), "one\ntwo!");
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod document;
pub mod editor;
pub mod history;
pub mod hit;
pub mod ime;
pub mod select;
pub mod text;

pub use crate::editor::{Command, Editor, Response};
pub use crate::history::History;
pub use crate::hit::{Caret, Hit, LineMap};
pub use crate::ime::Composition;
pub use crate::select::{Affinity, Selection};
pub use crate::text::{EditText, Splice};
