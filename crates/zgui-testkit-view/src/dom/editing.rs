//! The editing models over the editable elements of a recorded tree.
//!
//! # Why a component harness has to have one
//!
//! Typing is not something a component in this framework does. A key reaching an editable element
//! is handled by the framework *after* every listener on the path has passed on it: the model
//! inserts the character, writes the text into the tree, and announces the new value as an input
//! event, which is the only thing a component ever hears. A harness with no such step delivers a
//! key, runs the component's listeners, and stops — so every field is silent, and a component that
//! kept a private copy of the text to work around exactly that would pass every test here while
//! showing nothing but its placeholder in a window.
//!
//! So the step is here, over [`zgui_edit::Editor`] — the same model the runtime drives, not a
//! second one written to suit a test. What differs is only where the paragraphs are written: a
//! window projects them into document text nodes, and this projects them into the tree's.

use core::ops::Range;
use std::cell::RefCell;
use std::collections::HashMap;

use zgui_edit::{Editor, Selection};
use zgui_view::{Dom, NodeId};
use zgui_vocab::{Key, KeyEvent, Modifiers, NamedKey, UiState};

use crate::dom::recording::RecordingDom;

/// The elements this vocabulary lets a person type into.
const EDITABLE: [&str; 2] = ["editor", "field"];

/// The one editable element a line break can be typed into.
///
/// A single-line field must leave <kbd>Enter</kbd> alone, because Enter in a form is what submits
/// it: a field that swallowed the key would take a line break nothing displays and leave the form
/// unsendable from the keyboard.
const TAKES_LINE_BREAKS: &str = "editor";

/// One element's editing model, and the text nodes it writes through.
struct Attached {
    /// The model.
    editor: Editor,
    /// Which text node holds which paragraph.
    nodes: Vec<NodeId>,
}

/// Every editable element that has been typed into, by node.
#[derive(Default)]
pub(crate) struct Editing {
    /// The models, by the element they belong to.
    attached: RefCell<HashMap<NodeId, Attached>>,
}

/// What an event did to an editable element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edited {
    /// Whether the model took the event, which is what stops the same key also activating.
    pub handled: bool,
    /// Where the caret or the selection ended up.
    pub selection: Range<usize>,
    /// The whole text afterwards, when the event changed it.
    ///
    /// `None` when only the caret moved, because that is the difference between an event that
    /// reports a new value and one that reports none: a field announcing an input event for an
    /// arrow key would make every keystroke look like an edit to whatever is listening.
    pub value: Option<String>,
}

impl RecordingDom {
    /// Whether `node` is an element a person can type into.
    ///
    /// Either of disabled and read-only alone is enough to refuse the key. Asking whether *both*
    /// are set makes a disabled field writable, which is the whole of what disabling one is for.
    pub fn is_editable(&self, node: NodeId) -> bool {
        self.holds_text(node)
            && !self
                .tree()
                .ui_state(node)
                .intersects(UiState::DISABLED | UiState::READ_ONLY)
    }

    /// Whether `node` is an element this vocabulary keeps an editing model for.
    ///
    /// The kind alone, without asking whether a person may type into it: a disabled or read-only
    /// field still has a value, and an application still drives it.
    pub fn holds_text(&self, node: NodeId) -> bool {
        self.tree()
            .element_name(node)
            .is_some_and(|name| EDITABLE.contains(&name.as_str()))
    }

    /// Where `node`'s own model has its caret, or nothing when nothing has reached it yet.
    pub fn editing_selection(&self, node: NodeId) -> Option<Range<usize>> {
        self.editing()
            .attached
            .borrow()
            .get(&node)
            .map(|held| held.editor.selection().range())
    }

    /// Types one key into `node`, writing whatever changed into the tree.
    pub fn type_key(&self, node: NodeId, event: &KeyEvent, modifiers: Modifiers) -> Edited {
        if !self.is_editable(node) {
            return Edited::default();
        }
        // A key the element refuses is reported as untaken, so the framework's own behaviour for it
        // still runs — which is what leaves Enter to the form a single-line field is on.
        if matches!(event.key, Key::Named(NamedKey::Enter))
            && self.tree().element_name(node).as_deref() != Some(TAKES_LINE_BREAKS)
        {
            return Edited::default();
        }
        self.with_model(node, |editor| {
            let response = editor.key(event, modifiers);
            Edited {
                handled: response.handled,
                selection: editor.selection().range(),
                value: response.splice.is_some().then(|| editor.text()),
            }
        })
    }

    /// Puts `text` in `node`, as the value of a field its application owns.
    ///
    /// Text the element already holds does nothing at all and says so, which is what lets the same
    /// call be made on every change of the signal driving a controlled field: the echo of the
    /// user's own keystroke must not throw away the caret they are typing at. Text that really is
    /// different keeps the caret where it was, clamped into the new text, because an application
    /// that transforms what it was told writes back a different string on the very keystroke it
    /// was told about.
    ///
    /// It applies to a disabled or read-only element too: that is its application's own value, not
    /// something a person is typing.
    ///
    /// Answers whether the element's text is now not what it was.
    pub fn load_value(&self, node: NodeId, text: &str) -> bool {
        if !self.holds_text(node) {
            return false;
        }
        let text = text.to_owned();
        self.with_model(node, move |editor| {
            if editor.text() == text {
                return false;
            }
            let caret = editor.selection();
            editor.load(&text);
            // Clamped by the model itself, against the text that is now there.
            editor.set_selection(Selection::new(caret.anchor, caret.focus));
            true
        })
    }

    /// Runs `act` against `node`'s model and writes the paragraphs it left behind into the tree.
    ///
    /// The model is taken out of the map for the duration. Writing a text node runs whatever is
    /// observing it, and a model still borrowed while that happens is a panic in a test that did
    /// nothing wrong.
    fn with_model<T>(&self, node: NodeId, act: impl FnOnce(&mut Editor) -> T) -> T {
        let editing = self.editing();
        let taken = editing.attached.borrow_mut().remove(&node);
        // Built with nothing borrowed, because adopting an element with no text node writes one
        // into the tree.
        let mut held = match taken {
            Some(held) => held,
            None => Attached::adopt(self, node),
        };
        let answer = act(&mut held.editor);
        let paragraphs: Vec<String> = held.editor.paragraphs().to_vec();
        held.nodes = self.project(node, core::mem::take(&mut held.nodes), &paragraphs);
        editing.attached.borrow_mut().insert(node, held);
        answer
    }

    /// Writes `paragraphs` into `node`'s text children, one node each, and reports the nodes used.
    ///
    /// The nodes already there are written to rather than replaced: they are what the element was
    /// built with, and rebuilding them would make every keystroke a different tree for a component
    /// that changed nothing.
    fn project(&self, node: NodeId, mut nodes: Vec<NodeId>, paragraphs: &[String]) -> Vec<NodeId> {
        for (index, paragraph) in paragraphs.iter().enumerate() {
            match nodes.get(index) {
                Some(text) => self.set_text(*text, paragraph),
                None => {
                    let text = self.create_text(paragraph);
                    self.insert(node, text, None);
                    nodes.push(text);
                }
            }
        }
        for text in nodes.split_off(paragraphs.len()) {
            self.detach(text);
        }
        nodes
    }
}

impl Attached {
    /// Builds a model over the text an element already holds.
    ///
    /// The text nodes are adopted rather than rebuilt, and an element with no text node at all is
    /// given one: the buffer is never empty — no text is one empty paragraph, which is what makes
    /// "the caret is at offset zero" expressible — so a projection short of a node would drop every
    /// write into it. That is a blank field, which is what every field on a form starts as.
    fn adopt(dom: &RecordingDom, node: NodeId) -> Self {
        let mut nodes = Vec::new();
        let mut paragraphs = Vec::new();
        for child in dom.tree().children(node) {
            if let Some(text) = dom.tree().text(child) {
                nodes.push(child);
                paragraphs.push(text);
            }
        }
        let editor = Editor::new(&paragraphs.join("\n"));
        for _ in nodes.len()..editor.paragraphs().len() {
            let text = dom.create_text("");
            dom.insert(node, text, None);
            nodes.push(text);
        }
        Self { editor, nodes }
    }
}
