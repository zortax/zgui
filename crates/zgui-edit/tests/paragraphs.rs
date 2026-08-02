//! What a keystroke in a long document costs, measured through a real document and a real cache.
//!
//! The failure this exists to catch is smooth: an editing model that keeps its text as one string,
//! or writes every paragraph back into the document on every change, is correct in every visible
//! way and re-shapes the whole document per keystroke. No golden and no transcript can see it. Two
//! counts can — how many paragraphs the document has, and how many of them a keystroke re-shapes —
//! and the first is what stops the second from passing for the wrong reason: a document that is one
//! paragraph also re-shapes exactly one.

use std::sync::Arc;

use zgui_dom::{Document, EverythingMatters, NodeIndex};
use zgui_edit::Editor;
use zgui_edit::document::Projection;
use zgui_edit::editor::Command;
use zgui_edit::select::Selection;
use zgui_geom::CssPx;
use zgui_interned::ElementName;
use zgui_profile::Counter;
use zgui_scene::PaintSlot;
use zgui_testkit_scene::MonoShaper;
use zgui_testkit_scene::counters::Recording;
use zgui_text::{BreakRequest, ParagraphCache, ParagraphContent, StyledRun, TextMap, lay_out};
use zgui_text_style::{ParagraphStyle, TextStyle};

/// How many lines the textarea holds.
const LINES: usize = 200;

/// The width every paragraph is broken at.
const WIDTH: CssPx = CssPx(400.0);

/// The document, the projection of the editor's paragraphs into it, and the editor.
struct Textarea {
    /// The real document the paragraphs live in.
    document: Document,
    /// Which text node holds which paragraph.
    projection: Projection,
    /// The editing model.
    editor: Editor,
}

impl Textarea {
    /// A textarea holding [`LINES`] lines of text, projected into a real document.
    fn new() -> Self {
        let text: String = (0..LINES)
            .map(|line| format!("line {line} of the document\n"))
            .collect::<String>();
        let editor = Editor::new(text.trim_end_matches('\n'));
        let document = Document::new();
        let projection = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let field = edit.create_element(ElementName::new("editor"));
                edit.insert_before(root, field, None);
                Projection::build(edit, field, editor.buffer())
            })
            .expect("the document is not poisoned");
        Self {
            document,
            projection,
            editor,
        }
    }

    /// Lays out every paragraph *as the document holds it*.
    ///
    /// The text comes back out of the document rather than out of the editor, which is what makes
    /// this a measurement of the projection as well as of the model: a projection that rewrote
    /// every node would be invisible from the editor's side and is not from here.
    fn lay_out(&self, shaper: &mut MonoShaper, cache: &mut ParagraphCache<MonoLayout>) {
        let style = Arc::new(TextStyle::initial());
        let paragraph = ParagraphStyle::initial();
        for node in self.projection.nodes() {
            let text = paragraph_text(&self.document, *node);
            let map = TextMap::new();
            let runs = [StyledRun {
                text: 0..text.len(),
                style: style.clone(),
                brush: PaintSlot(0),
            }];
            let content = ParagraphContent {
                text: &text,
                map: &map,
                runs: &runs,
                boxes: &[],
                paragraph: &paragraph,
                scale: 1.0,
            };
            let request = BreakRequest::new(&content, Some(WIDTH));
            lay_out(shaper, cache, &content, &request);
        }
    }

    /// Types `text` into the middle paragraph, writing the change into the document.
    fn keystroke(&mut self, text: &str) {
        let middle = LINES / 2;
        let at = self.editor.buffer().start_of(middle) + 2;
        self.editor.set_selection(Selection::caret(at));
        let response = self.editor.apply(Command::Insert(text.to_owned()));
        let splice = response.splice.expect("typing changed the text");
        assert_eq!(splice.removed, middle..middle + 1, "one paragraph changed");
        let projection = &mut self.projection;
        let buffer = self.editor.buffer();
        self.document
            .edit(&EverythingMatters, |edit| {
                projection.apply(edit, &splice, buffer);
            })
            .expect("the document is not poisoned");
    }
}

/// The shaper's own held form, named so the cache can be declared.
type MonoLayout = zgui_testkit_scene::MonoLayout;

/// The text one node of the document holds.
fn paragraph_text(document: &Document, node: NodeIndex) -> String {
    zgui_dom::text::text_of(document.store(), node)
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_keystroke_in_a_200_line_textarea_reshapes_one_paragraph() {
    let mut recording = Recording::begin();
    let mut textarea = Textarea::new();
    let mut shaper = MonoShaper::new();
    let mut cache = ParagraphCache::new();

    assert_eq!(
        textarea.projection.nodes().len(),
        LINES,
        "one text node per paragraph, or the count below means nothing"
    );

    // The control: laying the document out for the first time shapes every paragraph, which is
    // both the proof that the counter moves at all and the cost the keystroke has to avoid.
    let first = recording.measure(|| textarea.lay_out(&mut shaper, &mut cache));
    first.assert_exactly(Counter::TextShaped, LINES as u64);

    let nodes_before = textarea.projection.nodes().to_vec();
    let typed = recording.measure(|| {
        textarea.keystroke("X");
        textarea.lay_out(&mut shaper, &mut cache);
    });
    typed.assert_exactly(Counter::TextShaped, 1);

    // A node holds its paragraph and the break that ends it: the paragraphs are separate nodes
    // with nothing between them, so a node that dropped the break would be laid out as one
    // continuous line with the paragraph after it.
    assert_eq!(
        paragraph_text(&textarea.document, textarea.projection.nodes()[LINES / 2]),
        "liXne 100 of the document\n",
        "the paragraph the caret was in is the one that changed"
    );
    assert_eq!(
        paragraph_text(&textarea.document, textarea.projection.nodes()[0]),
        "line 0 of the document\n",
        "and no other paragraph was rewritten"
    );
    assert_eq!(
        textarea.projection.nodes(),
        nodes_before,
        "and no node was torn down and made again, which would re-shape from a cold cache"
    );
}

#[test]
fn a_line_break_adds_one_node_and_leaves_every_other_paragraph_alone() {
    // The splice that changes the paragraph *count* is the one a projection gets wrong: writing
    // every node from the paragraph list afterwards is correct and costs a re-shape of the whole
    // document, and nothing but the node identities can tell the two apart.
    let mut textarea = Textarea::new();
    let before = textarea.projection.nodes().to_vec();
    textarea.keystroke("\n");

    let after = textarea.projection.nodes();
    assert_eq!(after.len(), LINES + 1);
    assert_eq!(
        &after[..LINES / 2],
        &before[..LINES / 2],
        "before the break"
    );
    assert_eq!(
        &after[LINES / 2 + 2..],
        &before[LINES / 2 + 1..],
        "after it, shifted by the one node that was added"
    );
    assert_eq!(paragraph_text(&textarea.document, after[LINES / 2]), "li\n");
    assert_eq!(
        paragraph_text(&textarea.document, after[LINES / 2 + 1]),
        "ne 100 of the document\n"
    );
}

#[test]
fn re_laying_out_an_unchanged_document_shapes_nothing() {
    // The other half of the same property: the cache is keyed by content, so a second pass over a
    // document nothing touched must cost no shaping at all. Without this, a keystroke costing one
    // shape would also be true of a pipeline that re-shaped one paragraph *every* frame.
    let mut recording = Recording::begin();
    let textarea = Textarea::new();
    let mut shaper = MonoShaper::new();
    let mut cache = ParagraphCache::new();

    let first = recording.measure(|| textarea.lay_out(&mut shaper, &mut cache));
    let control = first.control(Counter::TextShaped);

    let again = recording.measure(|| textarea.lay_out(&mut shaper, &mut cache));
    again.assert_zero(Counter::TextShaped, &control);
}
