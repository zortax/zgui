//! The editing model: the text, where the caret is, what can be undone, and what is being composed.

pub mod command;
pub mod keys;

use core::ops::Range;

use zgui_vocab::{ImeEvent, KeyEvent, Modifiers};

pub use crate::editor::command::{Command, Response};

use crate::history::{EditKind, Entry, History};
use crate::ime::Composition;
use crate::select::{Granularity, Motion, Selection, grapheme, motion, word};
use crate::text::EditText;

/// One editable field or document.
///
/// Everything about the text is here and nothing about how it is drawn: the model is asked what to
/// do, answers what changed, and never reaches for a layout. What needs the layout — where a click
/// landed, where the caret is on the screen — is [`crate::hit`], which takes the model's offsets
/// and the shaper's clusters and belongs to neither.
///
/// ```
/// use zgui_edit::Editor;
///
/// let mut editor = Editor::new("hello");
/// editor.set_selection(zgui_edit::select::Selection::caret(5));
/// editor.apply(zgui_edit::editor::Command::Insert(" there".to_owned()));
/// assert_eq!(editor.text(), "hello there");
///
/// editor.apply(zgui_edit::editor::Command::Undo);
/// assert_eq!(editor.text(), "hello", "one undo, however many letters it was");
/// ```
#[derive(Clone, Debug)]
pub struct Editor {
    /// The text, split into paragraphs.
    text: EditText,
    /// Where the caret is.
    selection: Selection,
    /// What can be undone.
    history: History,
    /// The composition in progress, if an input method has one.
    composing: Option<Composition>,
}

impl Editor {
    /// An editor over `text`, with the caret at the start and nothing to undo.
    pub fn new(text: &str) -> Self {
        Self {
            text: EditText::of(text),
            selection: Selection::caret(0),
            history: History::new(),
            composing: None,
        }
    }

    /// The text.
    pub fn text(&self) -> String {
        self.text.text()
    }

    /// The paragraphs, which is what a shaper is handed one of at a time.
    pub fn paragraphs(&self) -> &[String] {
        self.text.paragraphs()
    }

    /// The buffer.
    pub fn buffer(&self) -> &EditText {
        &self.text
    }

    /// Where the caret is, or what is selected.
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// What can be undone.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// The composition an input method has in progress, if any.
    pub fn composition(&self) -> Option<&Composition> {
        self.composing.as_ref()
    }

    /// Whether an input method is composing right now.
    ///
    /// While it is, key events are refused: the platform keeps delivering the keys the input
    /// method did not consume, and acting on one moves the caret out from under the provisional
    /// text so that the commit lands in the wrong place.
    pub fn is_composing(&self) -> bool {
        self.composing.is_some()
    }

    /// Puts the selection exactly here, clamped to the text.
    ///
    /// This also ends the current undo entry: a caret that was moved deliberately is a boundary a
    /// person expects one undo to stop at.
    pub fn set_selection(&mut self, selection: Selection) {
        let end = self.text.len();
        self.put(Selection {
            anchor: self.text.clamp(selection.anchor.min(end)),
            focus: self.text.clamp(selection.focus.min(end)),
            affinity: selection.affinity,
        });
        self.history.seal();
    }

    /// Records a selection, with the caret put at the one place it can be drawn.
    ///
    /// Everything that moves the caret goes through here, so that the affinity stored is one the
    /// text supports. An offset at the start of a paragraph other than the first has a hard break
    /// immediately before it, and a break is drawn at the end of the line it ends — so the upstream
    /// reading of that offset asks for a caret at the end of the previous line, which is not where
    /// that offset is. Typing <kbd>Enter</kbd> and finding the caret still on the line above is
    /// exactly that reading.
    ///
    /// Every other offset keeps the affinity it was given. A soft wrap and a direction boundary
    /// have two real places each, and which one is meant is what the caller knows and this does
    /// not.
    fn put(&mut self, selection: Selection) -> Selection {
        let position = self.text.position_of(selection.focus);
        let after_a_break = position.paragraph > 0 && position.offset == 0;
        self.selection = if after_a_break {
            selection.with_affinity(crate::select::Affinity::Downstream)
        } else {
            selection
        };
        self.selection
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.set_selection(Selection::new(0, self.text.len()));
    }

    /// Replaces the text and the selection, forgetting what could be undone.
    ///
    /// This is loading a value into a field rather than editing one: an undo across it would put
    /// back text the field was never showing, so the history is dropped rather than extended, and
    /// a composition in progress is dropped with it because the text it was displacing is gone.
    ///
    /// The response carries the splice, exactly as an edit's does, because the paragraphs a load
    /// changed still have to be written out to whatever is displaying them. A caller that wants to
    /// keep the caret where the person left it puts it back with
    /// [`set_selection`](Editor::set_selection): the caret here is at the start, which is where a
    /// field that has never been typed into has it.
    ///
    /// ```
    /// use zgui_edit::Editor;
    ///
    /// let mut editor = Editor::new("hello");
    /// editor.apply(zgui_edit::editor::Command::Insert("x".to_owned()));
    /// let response = editor.load("world");
    /// assert_eq!(editor.text(), "world");
    /// assert!(response.splice.is_some(), "the paragraph that changed is reported");
    ///
    /// editor.apply(zgui_edit::editor::Command::Undo);
    /// assert_eq!(editor.text(), "world", "an undo across a load is not an undo of it");
    /// ```
    pub fn load(&mut self, text: &str) -> Response {
        let splice = self.text.replace(0..self.text.len(), text);
        self.put(Selection::caret(0));
        self.history.clear();
        self.composing = None;
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(self.selection),
            clipboard: None,
        }
    }

    /// Does one thing to the text.
    pub fn apply(&mut self, command: Command) -> Response {
        match command {
            Command::Insert(text) => self.replace_selection(&text, EditKind::Insert),
            Command::DeleteBackwards(granularity) => self.delete(granularity, false),
            Command::DeleteForwards(granularity) => self.delete(granularity, true),
            Command::Move(motion) => self.move_caret(motion),
            Command::Select(selection) => {
                let before = self.selection;
                self.set_selection(selection);
                Response {
                    handled: true,
                    selection: (before != self.selection).then_some(self.selection),
                    ..Response::default()
                }
            }
            Command::SelectAll => {
                let before = self.selection;
                self.select_all();
                Response {
                    handled: true,
                    selection: (before != self.selection).then_some(self.selection),
                    ..Response::default()
                }
            }
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Copy => Response {
                handled: true,
                clipboard: self.selected_text(),
                ..Response::default()
            },
            Command::Cut => {
                let clipboard = self.selected_text();
                let mut response = if clipboard.is_some() {
                    self.replace_selection("", EditKind::Replace)
                } else {
                    Response::handled()
                };
                response.clipboard = clipboard;
                response
            }
            Command::Paste(text) => self.replace_selection(&text, EditKind::Replace),
        }
    }

    /// Does what a key press means, unless an input method is composing.
    ///
    /// A refused key is reported as not handled, so whatever else is listening for it — an escape
    /// that closes a dialog, a shortcut on a toolbar — still hears it. What must not happen is the
    /// *editor* acting on it.
    ///
    /// A composition showing *no* provisional text is not one to refuse a key for, and this is the
    /// difference between a field and a field that stops working. The window system clears the
    /// preedit before it commits and clears it again when a composition is abandoned, and the two
    /// are the same event: an empty preedit with nothing behind it. A commit follows the first
    /// immediately, with no key in between, so nothing is released too early — and refusing keys
    /// for the second would refuse them for ever, because the abandonment is never announced again.
    pub fn key(&mut self, event: &KeyEvent, modifiers: Modifiers) -> Response {
        if self.is_composing() && !self.release_empty_composition() {
            return Response::ignored();
        }
        match keys::command(event, modifiers) {
            Some(command) => self.apply(command),
            None => Response::ignored(),
        }
    }

    /// Advances the composition an input method is running.
    pub fn ime(&mut self, event: &ImeEvent) -> Response {
        match event {
            ImeEvent::Enabled => {
                self.history.seal();
                Response::handled()
            }
            ImeEvent::Preedit { text, cursor } => self.preedit(text.as_str(), cursor.clone()),
            ImeEvent::Commit(text) => self.commit(text.as_str()),
            ImeEvent::Disabled => self.abandon(),
            // A stage this build has never heard of leaves the composition exactly as it is: the
            // provisional text is still on the screen and the commit that follows still knows
            // where it is.
            _ => Response::ignored(),
        }
    }

    /// Finishes a composition on the text it is showing, as if the input method had committed it.
    ///
    /// This is what a field does when it stops being typed into with a composition still open —
    /// the window losing keyboard focus is the ordinary way — and it keeps what is on the screen.
    /// Discarding it instead would take back text the person can see and believes they have typed,
    /// and the input method is gone by then, so nothing will ever announce either outcome.
    ///
    /// One change is recorded, from the text the composition displaced to the text it is showing,
    /// so a single undo takes the whole composition back. A field composing nothing reports that
    /// it did nothing.
    ///
    /// ```
    /// use zgui_edit::Editor;
    /// use zgui_vocab::ImeEvent;
    ///
    /// let mut editor = Editor::new("ab");
    /// editor.ime(&ImeEvent::Preedit { text: "に".into(), cursor: None });
    /// assert!(editor.is_composing());
    ///
    /// editor.end_composition();
    /// assert_eq!(editor.text(), "にab", "what was on the screen stayed there");
    /// assert!(!editor.is_composing(), "and the composition is over");
    /// ```
    pub fn end_composition(&mut self) -> Response {
        let Some(composition) = self.composing.as_ref() else {
            return Response::ignored();
        };
        let showing = self.text.slice(composition.range.clone());
        self.commit(&showing)
    }

    /// Lets go of a composition that is showing nothing, so a key may be acted on.
    ///
    /// Answers whether it did. A composition that displaced text is kept even when empty, because
    /// letting go of it there would leave what it displaced deleted with nothing holding the
    /// original; that one is released by the dismissal the platform still owes.
    fn release_empty_composition(&mut self) -> bool {
        let releasable = self.composing.as_ref().is_some_and(|composition| {
            composition.range.is_empty() && composition.restore.is_empty()
        });
        if releasable {
            self.composing = None;
        }
        releasable
    }

    /// The selected text, or nothing when nothing is selected.
    fn selected_text(&self) -> Option<String> {
        (!self.selection.is_caret()).then(|| self.text.slice(self.selection.range()))
    }

    /// Replaces what is selected, recording one change.
    fn replace_selection(&mut self, with: &str, kind: EditKind) -> Response {
        let range = self.selection.range();
        self.replace(range, with, kind)
    }

    /// Replaces a range, recording one change and putting the caret after it.
    fn replace(&mut self, range: Range<usize>, with: &str, kind: EditKind) -> Response {
        let before = self.selection;
        let removed = self.text.slice(range.clone());
        if removed.is_empty() && with.is_empty() {
            return Response::handled();
        }
        let start = self.text.clamp(range.start);
        let splice = self.text.replace(range.clone(), with);
        let after = self.put(Selection::caret(start + with.len()));
        self.history.record(Entry {
            range: start..start + removed.len(),
            removed,
            inserted: with.to_owned(),
            before,
            after,
            kind,
        });
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(after),
            clipboard: None,
        }
    }

    /// Removes the selection, or one unit beside the caret.
    fn delete(&mut self, granularity: Granularity, forwards: bool) -> Response {
        if !self.selection.is_caret() {
            return self.replace_selection("", EditKind::Replace);
        }
        let content = self.text.text();
        let at = self.selection.focus;
        let other = match (granularity, forwards) {
            (Granularity::Word, true) => word::next(&content, at),
            (Granularity::Word, false) => word::previous(&content, at),
            (Granularity::Paragraph | Granularity::Document, true) => {
                motion::apply(
                    &self.text,
                    self.selection,
                    Motion::new(granularity, true, false),
                )
                .focus
            }
            (Granularity::Paragraph | Granularity::Document, false) => {
                motion::apply(
                    &self.text,
                    self.selection,
                    Motion::new(granularity, false, false),
                )
                .focus
            }
            (_, true) => grapheme::next(&content, at),
            (_, false) => grapheme::previous(&content, at),
        };
        if other == at {
            return Response::handled();
        }
        let kind = if forwards {
            EditKind::DeleteForwards
        } else {
            EditKind::DeleteBackwards
        };
        self.replace(at.min(other)..at.max(other), "", kind)
    }

    /// Moves the caret.
    fn move_caret(&mut self, wanted: Motion) -> Response {
        let before = self.selection;
        let moved = motion::apply(&self.text, self.selection, wanted);
        self.put(moved);
        if self.selection != before {
            self.history.seal();
        }
        Response {
            handled: true,
            selection: (self.selection != before).then_some(self.selection),
            ..Response::default()
        }
    }

    /// Takes back the last change.
    fn undo(&mut self) -> Response {
        let Some(entry) = self.history.undo() else {
            return Response::handled();
        };
        let splice = self.text.replace(entry.inserted_range(), &entry.removed);
        let selection = self.put(entry.before);
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(selection),
            clipboard: None,
        }
    }

    /// Puts back the last change that was taken back.
    fn redo(&mut self) -> Response {
        let Some(entry) = self.history.redo() else {
            return Response::handled();
        };
        let splice = self.text.replace(entry.range.clone(), &entry.inserted);
        let selection = self.put(entry.after);
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(selection),
            clipboard: None,
        }
    }

    /// Puts provisional text where the composition is, starting one if there is none.
    ///
    /// The provisional text is *not* recorded: an input method rewrites what it offered on every
    /// keystroke, and one undo entry per rewrite would make undoing a composed word take as many
    /// presses as composing it did. The single change is recorded when the composition commits.
    fn preedit(&mut self, text: &str, cursor: Option<Range<usize>>) -> Response {
        let composition = self.composing.take().unwrap_or_else(|| {
            let range = self.selection.range();
            let replaced = self.text.slice(range.clone());
            self.history.seal();
            Composition::started(self.text.clamp(range.start), replaced, self.selection)
        });
        let splice = self.text.replace(composition.range.clone(), text);
        let range = composition.range.start..composition.range.start + text.len();
        let composition = Composition {
            range,
            ..composition
        };
        let selection = self.put(composition.caret_for(cursor));
        // The composition stays alive even when the provisional text is momentarily empty: an
        // input method that clears its preedit and then commits is ordinary, and dropping the
        // composition there would let a key event in between move the caret the commit lands at.
        self.composing = Some(composition);
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(selection),
            clipboard: None,
        }
    }

    /// Finishes the composition, recording the whole of it as one change.
    ///
    /// The committed text goes where the provisional text is, never where the caret is: a key
    /// event that arrived mid-composition may have been refused, but a *pointer* press or a
    /// programmatic selection can still have moved the caret, and the input method is committing
    /// what it has been showing.
    fn commit(&mut self, text: &str) -> Response {
        let Some(composition) = self.composing.take() else {
            return self.replace_selection(text, EditKind::Replace);
        };
        let splice = self.text.replace(composition.range.clone(), text);
        let after = self.put(Selection::caret(composition.range.start + text.len()));
        self.history.seal();
        self.history.record(Entry {
            range: composition.restore_range(),
            removed: composition.restore,
            inserted: text.to_owned(),
            before: composition.restore_selection,
            after,
            kind: EditKind::Replace,
        });
        self.history.seal();
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(after),
            clipboard: None,
        }
    }

    /// Abandons the composition, putting back what it displaced.
    fn abandon(&mut self) -> Response {
        let Some(composition) = self.composing.take() else {
            return Response::handled();
        };
        let restore = composition.restore.clone();
        let splice = self.text.replace(composition.range.clone(), &restore);
        self.put(composition.restore_selection);
        Response {
            handled: true,
            splice: Some(splice),
            selection: Some(self.selection),
            clipboard: None,
        }
    }
}
