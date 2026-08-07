//! The editing models of the editable elements in one window.
//!
//! An editable element's text lives in the document, one text node per paragraph, and the model
//! over it lives here. The model is attached the first time a key or an input method reaches the
//! element and is kept for as long as the element is: it holds the undo stack and the composition,
//! neither of which can be recovered from the text.
//!
//! Editing is a *default action*, in the sense [`zgui_input::FrameworkDefault`] uses: it happens
//! after every listener on the path has run, and only if none of them took responsibility for the
//! event. A field with an `on:key_down` handler that calls `prevent_default` types nothing, which
//! is what makes a numeric-only field writable by an application.

use rustc_hash::FxHashMap;
use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKey, NodeKind};
use zgui_edit::document::Projection;
use zgui_edit::{Editor, Response};
use zgui_vocab::{ImeEvent, KeyEvent, Modifiers};

/// The elements this vocabulary lets a person type into.
///
/// A stated list rather than a computed property, for the same reason focusability is one: an
/// element that could be typed into by accident is a worse defect than one that has to be named.
const EDITABLE: [&str; 2] = ["editor", "field"];

/// The one editable element a line break can be typed into.
///
/// The difference between the two is not a matter of degree. A single-line field must leave
/// <kbd>Enter</kbd> alone, because Enter in a form is what submits it: a field that swallowed the
/// key would take a line break nothing displays and leave the form unsendable from the keyboard.
const TAKES_LINE_BREAKS: &str = "editor";

/// One element's editing model, and the text nodes it writes through.
struct Attached {
    /// The model.
    editor: Editor,
    /// Which text node holds which paragraph.
    projection: Projection,
    /// Whether the text has changed since the last time the value was reported as settled.
    ///
    /// Kept beside the model rather than derived from it, because "settled" is a question about
    /// what has already been announced and no amount of reading the text can answer it: a field
    /// left and returned to holds exactly the text it held before, and reporting it a second time
    /// makes every form validate twice.
    uncommitted: bool,
}

/// Every editable element that has been typed into, by node.
#[derive(Default)]
pub struct Editors {
    /// The models, by the element they belong to.
    attached: FxHashMap<NodeKey, Attached>,
}

impl core::fmt::Debug for Editors {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Editors")
            .field("attached", &self.attached.len())
            .finish()
    }
}

/// What an event did to an editable element.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edited {
    /// Whether the model took the event.
    pub handled: bool,
    /// Where the caret or selection ended up, when it moved.
    pub selection: Option<core::ops::Range<usize>>,
    /// Text the model asks to be placed on the clipboard.
    pub clipboard: Option<String>,
    /// Whether the model asks for the clipboard's text.
    ///
    /// The model can no more read the clipboard than write one: the request travels out to
    /// whoever holds the platform context, and the answer comes back through
    /// [`Editors::paste`].
    pub paste: bool,
    /// The whole text afterwards, when the event changed it.
    ///
    /// `None` when only the caret moved. This is what separates the event that reports a new value
    /// from the one that reports a new caret: an arrow key produces a selection and no value, and
    /// a field that announced a value change for it would make every keystroke look like an edit
    /// to whatever is listening.
    pub value: Option<String>,
}

/// What loading a value into an element did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loaded {
    /// Whether the element's text is not what it was.
    ///
    /// `false` when the value asked for is the one already there, which is the ordinary case of a
    /// controlled field being told what it just told its application.
    pub changed: bool,
    /// Where the caret ended up, when the text changed.
    pub selection: Option<core::ops::Range<usize>>,
}

impl Editors {
    /// Nothing attached to anything.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many elements have an editing model.
    pub fn len(&self) -> usize {
        self.attached.len()
    }

    /// Whether nothing has been typed into yet.
    pub fn is_empty(&self) -> bool {
        self.attached.is_empty()
    }

    /// The text an element's model holds, for a caller that wants to read a field's value.
    pub fn value(&self, node: NodeKey) -> Option<String> {
        self.attached.get(&node).map(|held| held.editor.text())
    }

    /// Forgets an element's model, which is what removing the element does.
    pub fn detach(&mut self, node: NodeKey) {
        self.attached.remove(&node);
    }

    /// Whether an element is one a person can type into.
    pub fn is_editable(document: &Document, node: NodeKey) -> bool {
        let Some(index) = document.store().index_of(node) else {
            return false;
        };
        Self::holds_text(document, node)
            // Either state alone is enough to refuse the key. Asking whether *both* are set makes
            // a disabled field writable, which is the whole of what disabling one is for.
            && !document
                .store()
                .core(index)
                .ui_state()
                .intersects(zgui_vocab::UiState::DISABLED | zgui_vocab::UiState::READ_ONLY)
    }

    /// Whether an element is one whose text this vocabulary keeps an editing model for.
    ///
    /// The kind alone, without asking whether a person may type into it. A disabled or read-only
    /// field still has a value, and an application still drives it: a form that fills a field in
    /// and then locks it, a field disabled while its request is in flight. Answering the narrower
    /// question here would leave every one of those showing whatever it happened to hold when it
    /// was locked, which is the state the value was *not* set to.
    pub fn holds_text(document: &Document, node: NodeKey) -> bool {
        let Some(index) = document.store().index_of(node) else {
            return false;
        };
        let record = document.store().core(index);
        record.kind() == NodeKind::Element && EDITABLE.contains(&record.local_name().as_str())
    }

    /// Where an element's own model has its caret, and which way its selection was made.
    ///
    /// The model's answer rather than the record beside it, because the record is an ascending byte
    /// range and has forgotten both the end the caret is at and which of the two places a boundary
    /// offset means. Painting a caret from the record puts it at the wrong end of every selection
    /// made backwards.
    pub fn selection(&self, node: NodeKey) -> Option<zgui_edit::Selection> {
        self.attached.get(&node).map(|held| held.editor.selection())
    }

    /// Puts an element's caret at `focus`, with `anchor` as the end that stays put.
    ///
    /// Separate from [`Editors::select`] because a range cannot say which end moves, and a drag
    /// upwards or leftwards moves the lower one: a selection recorded the other way round extends
    /// from the wrong end on the next shift-click.
    pub fn place(
        &mut self,
        document: &Document,
        node: NodeKey,
        anchor: usize,
        focus: usize,
        affinity: zgui_edit::Affinity,
    ) -> Edited {
        self.deliver(document, node, |editor| {
            editor.apply(zgui_edit::Command::Select(zgui_edit::Selection {
                anchor,
                focus,
                affinity,
            }))
        })
    }

    /// Puts an element's selection exactly here, in the offsets its own text is measured in.
    ///
    /// The model is attached if it was not already, because the whole point of writing a selection
    /// is what the next keystroke does to it: a selection recorded beside a model that is created
    /// afterwards, with its caret at the start, types in the wrong place.
    pub fn select(
        &mut self,
        document: &Document,
        node: NodeKey,
        range: core::ops::Range<usize>,
    ) -> Edited {
        self.place(
            document,
            node,
            range.start,
            range.end,
            zgui_edit::Affinity::Upstream,
        )
    }

    /// Types a key into an element, writing whatever changed into the document.
    ///
    /// A key the element refuses is reported as untaken, so the framework's own behaviour for it
    /// still runs — which is what leaves <kbd>Enter</kbd> to the form a single-line field is on.
    pub fn key(
        &mut self,
        document: &Document,
        node: NodeKey,
        event: &KeyEvent,
        modifiers: Modifiers,
    ) -> Edited {
        if Self::would_break_a_line(event) && !Self::takes_line_breaks(document, node) {
            return Edited::default();
        }
        self.deliver(document, node, |editor| editor.key(event, modifiers))
    }

    /// Whether this key would put a line break in.
    fn would_break_a_line(event: &KeyEvent) -> bool {
        matches!(
            event.key,
            zgui_vocab::Key::Named(zgui_vocab::NamedKey::Enter)
        )
    }

    /// Whether an element is one a line break can be typed into.
    fn takes_line_breaks(document: &Document, node: NodeKey) -> bool {
        let Some(index) = document.store().index_of(node) else {
            return false;
        };
        document.store().core(index).local_name().as_str() == TAKES_LINE_BREAKS
    }

    /// Advances an element's composition, writing whatever changed into the document.
    pub fn ime(&mut self, document: &Document, node: NodeKey, event: &ImeEvent) -> Edited {
        self.deliver(document, node, |editor| editor.ime(event))
    }

    /// Finishes an element's composition on the text it is showing.
    ///
    /// What a field does when it stops being typed into with a composition still open. The
    /// provisional text stays and becomes one undoable change; nothing is asked of the input
    /// method, because the reason this is reached is that the input method is no longer there.
    pub fn end_composition(&mut self, document: &Document, node: NodeKey) -> Edited {
        // Asked of the model that is already there rather than of one attached to answer it: an
        // element nobody has typed into is composing nothing, and building a model for it here
        // would put text nodes under every element the focus has ever rested on.
        if !self
            .attached
            .get(&node)
            .is_some_and(|held| held.editor.is_composing())
        {
            return Edited::default();
        }
        self.deliver(document, node, Editor::end_composition)
    }

    /// Puts `text` in an element, as the value of a field its application owns.
    ///
    /// This is the other direction from typing, and a field bound to a signal is nothing without
    /// it: the text of an editable element lives in the editing model and in the document, and a
    /// view can write neither — the model is the window's, and text nodes an editor projects
    /// through are the editor's.
    ///
    /// Three things make it usable as the echo of a controlled field rather than only as a reset.
    ///
    /// Text that is already what the element holds does **nothing at all**, and reports that it
    /// did. The ordinary controlled loop is a keystroke announced as an input event, an application
    /// writing its signal from it, and that signal arriving back here one frame later — and a load
    /// that rebuilt the text every time would throw away the caret and the undo stack on every
    /// letter typed.
    ///
    /// A load that *does* change the text keeps the caret where it was, clamped into the new text,
    /// rather than moving it to either end. An application that transforms what it was told — a
    /// field that upper-cases, one that strips spaces — writes back a different string on the very
    /// keystroke it was told about, and a caret sent to the start or the end there types the next
    /// letter in the wrong place.
    ///
    /// It applies to a disabled or read-only element too, because that is its application's own
    /// value and not something a person is typing.
    ///
    /// Nothing here marks the value as needing to be settled: an application that already knows
    /// what it wrote does not need to be told about it when the field is left, and a form that
    /// revalidated on every value it set itself would fight its own defaults.
    pub fn load(&mut self, document: &Document, node: NodeKey, text: &str) -> Loaded {
        if !Self::holds_text(document, node) {
            return Loaded::default();
        }
        let Some(index) = document.store().index_of(node) else {
            return Loaded::default();
        };
        let attached = self
            .attached
            .entry(node)
            .or_insert_with(|| Attached::adopt(document, index));
        if attached.editor.text() == text {
            return Loaded::default();
        }
        let caret = attached.editor.selection();
        let response = attached.editor.load(text);
        // Clamped by the model itself, against the text that is now there. Restored *after* the
        // load, because the model is what knows where a character boundary is now.
        attached
            .editor
            .set_selection(zgui_edit::Selection::new(caret.anchor, caret.focus));
        if let Some(splice) = &response.splice {
            let projection = &mut attached.projection;
            let buffer = attached.editor.buffer();
            let _ = document.edit(&EverythingMatters, |edit| {
                projection.apply(edit, splice, buffer);
            });
        }
        Loaded {
            changed: true,
            selection: Some(attached.editor.selection().range()),
        }
    }

    /// Runs one thing against an element's model and writes the result out.
    fn deliver(
        &mut self,
        document: &Document,
        node: NodeKey,
        act: impl FnOnce(&mut Editor) -> Response,
    ) -> Edited {
        if !Self::is_editable(document, node) {
            return Edited::default();
        }
        let Some(index) = document.store().index_of(node) else {
            return Edited::default();
        };
        let attached = self
            .attached
            .entry(node)
            .or_insert_with(|| Attached::adopt(document, index));

        let response = act(&mut attached.editor);
        let mut value = None;
        if let Some(splice) = &response.splice {
            let projection = &mut attached.projection;
            let buffer = attached.editor.buffer();
            // The write goes through the document's own batch, so the style engine is told what
            // changed by the same route every other mutation uses.
            let _ = document.edit(&EverythingMatters, |edit| {
                projection.apply(edit, splice, buffer);
            });
            value = Some(attached.editor.text());
            attached.uncommitted = true;
        }
        Edited {
            handled: response.handled,
            selection: response.selection.map(|selection| selection.range()),
            clipboard: response.clipboard,
            paste: response.paste,
            value,
        }
    }

    /// Pastes clipboard text into an element, replacing the selection as one undoable change.
    ///
    /// The other half of the request [`Edited::paste`] carries out: whoever read the clipboard
    /// hands the text back in here. Line breaks survive only where they can be typed — a
    /// single-line field joins the lines with spaces, because a break nothing displays would put
    /// a value in the field the user cannot see and cannot delete.
    pub fn paste(&mut self, document: &Document, node: NodeKey, text: &str) -> Edited {
        let text = if Self::takes_line_breaks(document, node) {
            text.to_owned()
        } else {
            text.split(['\r', '\n'])
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        // Text that boiled away entirely — an empty clipboard, a lone line break — pastes
        // nothing rather than replacing the selection with nothing, which would be a delete.
        if text.is_empty() {
            return Edited::default();
        }
        self.deliver(document, node, |editor| {
            editor.apply(zgui_edit::Command::Paste(text))
        })
    }

    /// The value an element has stopped being edited on, once, or nothing when it has not changed.
    ///
    /// Answered once per run of edits: leaving a field reports what was typed into it, and leaving
    /// it again without typing reports nothing. A caller that asked twice and acted twice would
    /// submit a form for every time the user looked at it.
    pub fn settle(&mut self, node: NodeKey) -> Option<String> {
        let attached = self.attached.get_mut(&node)?;
        if !core::mem::take(&mut attached.uncommitted) {
            return None;
        }
        Some(attached.editor.text())
    }
}

impl Attached {
    /// Builds a model over the text an element already holds.
    ///
    /// The text nodes are adopted rather than rebuilt: they were made by whatever built the view,
    /// they are already shaped, and replacing them would throw that away on the first keystroke.
    ///
    /// The value is the text under the element read straight through, because that is what the
    /// element says: a view writes a value as one text node however many lines it has, and a model
    /// built from one node per line would read a two-line value as one line of it.
    ///
    /// What is *written back* is one paragraph per node, each ending in its own break, and the
    /// element is brought to that shape here — nodes created for the paragraphs it is short of,
    /// nodes removed for the paragraphs it has too many of, and a node written only when what it
    /// holds is not what the paragraph is. Every keystroke afterwards writes one node. An element
    /// left in its original shape would have the first keystroke rewrite one paragraph's node and
    /// leave a node holding the whole original value beside it, so the field would show its first
    /// line twice and its edit once.
    ///
    /// The buffer is never empty — no text is one empty paragraph, because that is what makes "the
    /// caret is at offset 0" expressible — so an element with *no* text under it is short by one,
    /// and a projection that stayed short would drop every write into it. That is a blank field,
    /// which is the ordinary state of every field on a form the first time it is shown, and it
    /// types nothing at all.
    fn adopt(document: &Document, index: NodeIndex) -> Self {
        let store = document.store();
        let mut nodes = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut held = String::new();
        let mut child = store.core(index).first_child();
        while let Some(node) = child {
            if store.core(node).kind() == NodeKind::Text {
                let text = zgui_dom::text::text_of(store, node).unwrap_or_default();
                held.push_str(text);
                texts.push(text.to_owned());
                nodes.push(node);
            }
            child = store.core(node).next_sibling();
        }
        let editor = Editor::new(&held);
        let wanted = editor.paragraphs().len();
        let _ = document.edit(&EverythingMatters, |edit| {
            for node in nodes.drain(wanted.min(nodes.len())..) {
                edit.remove(node);
            }
            for _ in nodes.len()..wanted {
                let node = edit.create_text("");
                edit.insert_before(index, node, None);
                nodes.push(node);
                texts.push(String::new());
            }
            for (paragraph, node) in nodes.iter().enumerate() {
                let Some(content) = zgui_edit::document::content_of(editor.buffer(), paragraph)
                else {
                    continue;
                };
                // Written only when it differs: an element already in this shape is the ordinary
                // case, and setting the text a node already holds would re-shape every field the
                // focus has ever rested on.
                if texts.get(paragraph).is_none_or(|held| *held != content) {
                    edit.set_text(*node, &content);
                }
            }
        });
        Self {
            projection: Projection::adopt(index, nodes),
            editor,
            uncommitted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeIndex};
    use zgui_interned::ElementName;
    use zgui_vocab::{Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey};

    use super::Editors;

    /// A document holding one editable element with `paragraphs` in it.
    fn field(name: &str, paragraphs: &[&str]) -> (Document, NodeIndex) {
        let document = Document::new();
        let field = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let field = edit.create_element(ElementName::new(name));
                edit.insert_before(root, field, None);
                for paragraph in paragraphs {
                    let text = edit.create_text(paragraph);
                    edit.insert_before(field, text, None);
                }
                field
            })
            .expect("not poisoned");
        (document, field)
    }

    /// A press of a letter key.
    fn letter(text: &str) -> KeyEvent {
        KeyEvent {
            key: Key::Character(text.into()),
            key_without_modifiers: Key::Character(text.into()),
            physical: PhysicalKey::Code(KeyCode::KeyA),
            location: zgui_vocab::KeyLocation::Standard,
            repeat: false,
        }
    }

    /// The text one node holds.
    fn text_of(document: &Document, node: NodeIndex) -> String {
        zgui_dom::text::text_of(document.store(), node)
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn an_edit_reports_the_whole_new_value_and_a_caret_move_reports_none() {
        // The value is what tells the view layer what the field now holds. Reporting one for a
        // movement too would make every arrow key look like an edit to whatever is bound to it,
        // and reporting none at all would leave a field that types perfectly bound to nothing.
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();

        let edited = editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert_eq!(edited.value.as_deref(), Some("zab"));

        let right = KeyEvent::named(NamedKey::ArrowRight, PhysicalKey::Code(KeyCode::ArrowRight));
        let edited = editors.key(&document, key, &right, Modifiers::NONE);
        assert!(edited.handled, "the caret did move");
        assert_eq!(edited.value, None, "moving the caret reported a new value");
    }

    #[test]
    fn a_value_settles_once_and_a_field_nobody_touched_never_settles() {
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        assert_eq!(editors.settle(key), None, "nothing is even attached yet");

        editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert_eq!(editors.settle(key).as_deref(), Some("zab"));
        assert_eq!(
            editors.settle(key),
            None,
            "the same run of edits settled twice, so a form would submit twice"
        );

        // Moving the caret is not a change, so it does not make one settle again.
        let right = KeyEvent::named(NamedKey::ArrowRight, PhysicalKey::Code(KeyCode::ArrowRight));
        editors.key(&document, key, &right, Modifiers::NONE);
        assert_eq!(editors.settle(key), None);
    }

    #[test]
    fn an_element_with_no_text_under_it_is_given_a_node_to_write_through() {
        // A blank field is what every field on a form starts as. The buffer is never empty — no
        // text is one empty paragraph — so an element with no text node under it is one short, and
        // a projection that stayed short applies every keystroke to the buffer and drops it on the
        // way to the document: the caret advances, the reported value is right, and the screen
        // stays blank.
        let (document, field) = field("field", &[]);
        assert_eq!(
            document.store().core(field).first_child(),
            None,
            "the fixture is meant to have nothing under it"
        );
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        let edited = editors.key(&document, key, &letter("h"), Modifiers::NONE);
        assert_eq!(edited.value.as_deref(), Some("h"));

        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("a text node to write the typed letter into");
        assert_eq!(text_of(&document, node), "h");
        assert_eq!(
            document.store().core(node).next_sibling(),
            None,
            "one paragraph, so one text node"
        );
    }

    #[test]
    fn typing_into_a_field_writes_the_text_back_into_the_document() {
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();

        // The caret starts at the beginning, which is where a freshly attached model puts it.
        let edited = editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert!(edited.handled);
        assert_eq!(edited.selection, Some(1..1));

        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("the field's text node");
        assert_eq!(
            text_of(&document, node),
            "zab",
            "the document itself changed, not a copy of it beside it"
        );
    }

    #[test]
    fn a_second_keystroke_reaches_the_same_model_rather_than_a_fresh_one() {
        // A model rebuilt per event would type the second letter at the caret a fresh model starts
        // with, and would have no undo stack at all.
        let (document, field) = field("editor", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        editors.key(&document, key, &letter("x"), Modifiers::NONE);
        editors.key(&document, key, &letter("y"), Modifiers::NONE);
        assert_eq!(editors.value(key).as_deref(), Some("xyab"));
        assert_eq!(editors.len(), 1);
    }

    #[test]
    fn an_element_that_is_not_editable_is_never_typed_into() {
        let (document, element) = field("box", &["ab"]);
        let key = document.store().key_of(element);
        let mut editors = Editors::new();
        let edited = editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert!(!edited.handled);
        assert!(editors.is_empty());
    }

    #[test]
    fn a_line_break_typed_into_a_field_adds_a_paragraph_node() {
        let (document, field) = field("editor", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        let enter = KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
        editors.key(&document, key, &enter, Modifiers::NONE);

        let mut nodes = Vec::new();
        let mut child = document.store().core(field).first_child();
        while let Some(node) = child {
            nodes.push(node);
            child = document.store().core(node).next_sibling();
        }
        assert_eq!(nodes.len(), 2, "two paragraphs, so two text nodes");
        // The break that ends the first paragraph is written into that paragraph's own node: two
        // text nodes with nothing between them are laid out as one continuous line, so a break
        // that stayed in the model alone would leave the two lines drawn side by side.
        assert_eq!(text_of(&document, nodes[0]), "\n");
        assert_eq!(text_of(&document, nodes[1]), "ab");
    }

    #[test]
    fn a_disabled_or_read_only_field_is_never_typed_into() {
        // Either state on its own is enough. A predicate asking whether *both* are set is true of
        // neither field below and reads as if it refused both.
        for state in [
            zgui_vocab::UiState::DISABLED,
            zgui_vocab::UiState::READ_ONLY,
        ] {
            let (document, field) = field("field", &["ab"]);
            document
                .edit(&EverythingMatters, |edit| {
                    edit.set_state(field, state, true);
                })
                .expect("not poisoned");
            let key = document.store().key_of(field);
            let mut editors = Editors::new();
            let edited = editors.key(&document, key, &letter("z"), Modifiers::NONE);
            assert!(!edited.handled, "{state:?} took the key");
            let node = document
                .store()
                .core(field)
                .first_child()
                .expect("the field's text node");
            assert_eq!(text_of(&document, node), "ab", "{state:?} was typed into");
            assert_eq!(editors.value(key), None, "and no model was even attached");
        }
    }

    #[test]
    fn loading_a_value_writes_it_into_the_document_and_keeps_the_caret_where_it_was() {
        // The whole of a controlled field. An application owns the text, so the text has to be
        // drivable from outside — and the caret has to survive it, because the ordinary case is an
        // application writing back a *transformed* version of the very keystroke it was told about.
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();

        editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert_eq!(editors.value(key).as_deref(), Some("zab"));

        let loaded = editors.load(&document, key, "ZAB");
        assert!(loaded.changed);
        assert_eq!(
            loaded.selection,
            Some(1..1),
            "the caret was moved by the echo of the letter that was just typed"
        );
        assert_eq!(editors.value(key).as_deref(), Some("ZAB"));
        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("the field's text node");
        assert_eq!(
            text_of(&document, node),
            "ZAB",
            "the load never reached the document, so the field shows the old text"
        );

        // And the next letter goes where the caret is, which is the thing the caret is for.
        editors.key(&document, key, &letter("y"), Modifiers::NONE);
        assert_eq!(editors.value(key).as_deref(), Some("ZyAB"));
    }

    #[test]
    fn loading_the_value_a_field_already_holds_changes_nothing_at_all() {
        // The echo of an untransformed controlled field: every keystroke comes back as the value
        // that is already there. A load that rebuilt the text anyway would drop the caret and the
        // undo stack on every letter typed, which is a field that cannot be typed into quickly.
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        editors.key(&document, key, &letter("z"), Modifiers::NONE);

        assert_eq!(
            editors.load(&document, key, "zab"),
            super::Loaded::default()
        );

        let typed = editors.key(&document, key, &letter("q"), Modifiers::NONE);
        assert_eq!(
            typed.selection,
            Some(2..2),
            "the caret was reset by a load that was supposed to be a no-op"
        );
        assert_eq!(editors.value(key).as_deref(), Some("zqab"));
    }

    #[test]
    fn a_read_only_field_refuses_the_keyboard_and_still_takes_a_value_from_its_application() {
        // The two are different questions. A field locked while its request is in flight still has
        // to show what the application put in it, and a load that asked whether a *person* may type
        // here would leave every disabled field showing whatever it held when it was locked.
        let (document, field) = field("field", &["ab"]);
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_state(field, zgui_vocab::UiState::READ_ONLY, true);
            })
            .expect("not poisoned");
        let key = document.store().key_of(field);
        let mut editors = Editors::new();

        assert!(
            !editors
                .key(&document, key, &letter("z"), Modifiers::NONE)
                .handled
        );
        assert!(editors.load(&document, key, "filled in").changed);
        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("the field's text node");
        assert_eq!(text_of(&document, node), "filled in");
    }

    #[test]
    fn ending_a_composition_keeps_what_it_was_showing_and_lets_the_next_key_through() {
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        editors.ime(
            &document,
            key,
            &zgui_vocab::ImeEvent::Preedit {
                text: "に".into(),
                cursor: None,
            },
        );
        assert_eq!(editors.value(key).as_deref(), Some("にab"));

        let ended = editors.end_composition(&document, key);
        assert!(ended.handled);
        assert_eq!(editors.value(key).as_deref(), Some("にab"));
        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("the field's text node");
        assert_eq!(text_of(&document, node), "にab");

        // The composition is over, so a key is acted on rather than refused into it.
        let typed = editors.key(&document, key, &letter("z"), Modifiers::NONE);
        assert!(
            typed.handled,
            "the composition is still open, so every key from here is refused for ever"
        );
        assert_eq!(editors.value(key).as_deref(), Some("にzab"));
    }

    #[test]
    fn ending_a_composition_nobody_started_attaches_nothing_and_does_nothing() {
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        assert_eq!(
            editors.end_composition(&document, key),
            super::Edited::default()
        );
        assert!(editors.is_empty());
    }

    #[test]
    fn control_v_asks_for_the_clipboard_instead_of_typing_a_v() {
        // The request has to travel out, because the text is not here to paste: the clipboard
        // belongs to the platform, and the model can only ask.
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        let edited = editors.key(&document, key, &letter("v"), Modifiers::CONTROL);
        assert!(edited.handled, "the chord belongs to the field");
        assert!(edited.paste, "and it is a request for the clipboard");
        assert_eq!(edited.value, None, "nothing was typed yet");
    }

    #[test]
    fn a_paste_replaces_the_selection_and_reports_the_new_value() {
        let (document, field) = field("field", &["abc"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        editors.select(&document, key, 0..2);
        let edited = editors.paste(&document, key, "XY");
        assert!(edited.handled);
        assert_eq!(edited.value.as_deref(), Some("XYc"));
        assert_eq!(
            edited.selection,
            Some(2..2),
            "the caret sits after the paste"
        );
        let node = document
            .store()
            .core(field)
            .first_child()
            .expect("the field's text node");
        assert_eq!(text_of(&document, node), "XYc");
    }

    #[test]
    fn a_paste_into_a_single_line_field_joins_the_lines_and_an_editor_keeps_them() {
        // A field must not hold a break nothing displays; an editor is exactly where the breaks
        // belong.
        let (document, single) = field("field", &[]);
        let key = document.store().key_of(single);
        let mut editors = Editors::new();
        let edited = editors.paste(&document, key, "one\r\ntwo\nthree");
        assert_eq!(edited.value.as_deref(), Some("one two three"));

        let (document, multi) = field("editor", &[]);
        let key = document.store().key_of(multi);
        let mut editors = Editors::new();
        let edited = editors.paste(&document, key, "one\ntwo");
        assert_eq!(edited.value.as_deref(), Some("one\ntwo"));
    }

    #[test]
    fn pasting_nothing_at_all_leaves_the_selection_alone() {
        // Replacing the selection with an empty string is a delete, and a paste that found the
        // clipboard empty must not become one.
        let (document, field) = field("field", &["abc"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        editors.select(&document, key, 0..2);
        let edited = editors.paste(&document, key, "\n");
        assert_eq!(edited, super::Edited::default());
        assert_eq!(editors.value(key).as_deref(), Some("abc"));
    }

    #[test]
    fn a_key_the_model_has_no_use_for_is_left_for_everything_else() {
        let (document, field) = field("field", &["ab"]);
        let key = document.store().key_of(field);
        let mut editors = Editors::new();
        let escape = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
        assert!(
            !editors
                .key(&document, key, &escape, Modifiers::NONE)
                .handled
        );
    }
}
