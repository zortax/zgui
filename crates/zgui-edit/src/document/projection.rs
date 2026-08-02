//! The text nodes an editor's paragraphs are written into.

use zgui_dom::{Edit, NodeIndex};

use crate::text::{EditText, Splice};

/// The text one paragraph's node holds: the paragraph, and the break that ends it.
///
/// Every paragraph but the last is followed by a line break, and that break is written into the
/// same node the paragraph is. Nothing else in the document could carry it — there is one node per
/// paragraph and nothing between them — and text nodes that sit in one formatting context with
/// nothing between them are laid out as one continuous run of characters. A projection that wrote
/// the paragraphs bare would answer <kbd>Enter</kbd> by moving the text after the caret into a
/// second node and leaving the screen showing exactly the line it showed before.
///
/// Nothing when there is no such paragraph.
pub fn content_of(text: &EditText, paragraph: usize) -> Option<String> {
    let content = text.paragraph(paragraph)?;
    if paragraph + 1 < text.paragraphs().len() {
        Some(format!("{content}\n"))
    } else {
        Some(content.to_owned())
    }
}

/// Which text node holds which paragraph of an editor.
///
/// Built once against an editable element and updated by every splice. A splice that replaced one
/// paragraph with one writes one node and touches nothing else, which is what keeps a keystroke
/// from re-shaping the document.
///
/// A node holds its paragraph and the break that ends it, so the text under the element, read in
/// order, is the text the editor holds — see [`content_of`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Projection {
    /// The element the paragraphs are children of.
    root: Option<NodeIndex>,
    /// One text node per paragraph, in order.
    nodes: Vec<NodeIndex>,
}

impl Projection {
    /// Writes every paragraph of `text` into fresh text nodes under `root`.
    ///
    /// The element is expected to be empty; anything already under it stays where it is and is not
    /// part of the projection, because an editor owns the nodes it made and nothing else.
    pub fn build(edit: &mut Edit<'_>, root: NodeIndex, text: &EditText) -> Self {
        let nodes = (0..text.paragraphs().len())
            .map(|paragraph| {
                let content = content_of(text, paragraph).unwrap_or_default();
                let node = edit.create_text(&content);
                edit.insert_before(root, node, None);
                node
            })
            .collect();
        Self {
            root: Some(root),
            nodes,
        }
    }

    /// Takes over text nodes something else already made, one per paragraph, in order.
    ///
    /// This is how an editor attaches to an element a view built: the nodes are already there and
    /// already shaped, and making a fresh set would throw both away on the first keystroke.
    pub fn adopt(root: NodeIndex, nodes: Vec<NodeIndex>) -> Self {
        Self {
            root: Some(root),
            nodes,
        }
    }

    /// The element the paragraphs live under.
    pub fn root(&self) -> Option<NodeIndex> {
        self.root
    }

    /// The text nodes, one per paragraph.
    pub fn nodes(&self) -> &[NodeIndex] {
        &self.nodes
    }

    /// The node holding one paragraph.
    pub fn node(&self, paragraph: usize) -> Option<NodeIndex> {
        self.nodes.get(paragraph).copied()
    }

    /// Writes one splice's paragraphs into the document.
    ///
    /// Only the paragraphs the splice named are written. A splice that changed the paragraph
    /// *count* creates or removes exactly the difference, so the paragraphs on either side of the
    /// change keep the nodes — and therefore the shaped results — they already had.
    ///
    /// The break that ends a paragraph goes into that paragraph's own node, so a splice that made a
    /// paragraph the last one, or stopped it being the last one, writes it: both are inside the
    /// range a splice reports, because the paragraph whose position at the end of the text changed
    /// is one of the paragraphs the replacement touched.
    pub fn apply(&mut self, edit: &mut Edit<'_>, splice: &Splice, text: &EditText) {
        let Some(root) = self.root else {
            return;
        };
        let inserted = splice.inserted_range();
        let shared = splice.removed.len().min(splice.inserted);
        for paragraph in splice.removed.start..splice.removed.start + shared {
            if let (Some(node), Some(content)) = (self.node(paragraph), content_of(text, paragraph))
            {
                edit.set_text(node, &content);
            }
        }
        for paragraph in (splice.removed.start + shared)..splice.removed.end {
            let _ = paragraph;
            if let Some(node) = self.nodes.get(splice.removed.start + shared).copied() {
                edit.remove(node);
                self.nodes.remove(splice.removed.start + shared);
            }
        }
        for paragraph in (splice.removed.start + shared)..inserted.end {
            let Some(content) = content_of(text, paragraph) else {
                continue;
            };
            let node = edit.create_text(&content);
            let before = self.nodes.get(paragraph).copied();
            edit.insert_before(root, node, before);
            self.nodes.insert(paragraph.min(self.nodes.len()), node);
        }
    }
}
