//! Flattening the boxes inside one inline formatting context into a sequence.

use zgui_dom::side::BoxKey;

use crate::node::kind::FormattingContext;
use crate::tree::store::Structure;

/// One piece of an inline formatting context, in document order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Piece {
    /// A run of text, whose characters and style are the box's own.
    Text(BoxKey),
    /// An atomic inline or a replaced box.
    Atomic(BoxKey),
    /// A nested inline box is beginning: everything until its matching end is inside it.
    Enter(BoxKey),
    /// It is ending.
    Leave(BoxKey),
}

/// The pieces inside one inline formatting context, in document order.
///
/// Nested inline boxes are entered rather than descended past, because their edges take up space
/// on the line and their styles apply to everything between them. Everything else is a leaf: a run
/// of text contributes characters, and an atomic inline contributes one opaque box.
///
/// Boxes that generate nothing are skipped here rather than filtered later, so that a `display:
/// none` span leaves no trace in the string and therefore none in any offset mapped through it.
pub(crate) fn pieces(store: Structure<'_>, root: BoxKey) -> Vec<Piece> {
    let mut out = Vec::new();
    // A run of text that is block-level on its own — one that became a flex or grid item — is a
    // context whose whole content is itself.
    if store.node(root).text.is_some() {
        out.push(Piece::Text(root));
        return out;
    }
    for &child in &store.node(root).children {
        push(store, child, &mut out);
    }
    out
}

/// Appends one child and everything below it.
fn push(store: Structure<'_>, key: BoxKey, out: &mut Vec<Piece>) {
    let Some(node) = store.get(key) else {
        return;
    };
    match node.fc {
        FormattingContext::None => {}
        // A custom element in inline flow is an atom exactly as a replaced box is: the line asks
        // how big it is, and never looks in.
        FormattingContext::Atomic | FormattingContext::Replaced | FormattingContext::Custom => {
            out.push(Piece::Atomic(key))
        }
        FormattingContext::Inline if node.text.is_some() => out.push(Piece::Text(key)),
        FormattingContext::Inline => {
            out.push(Piece::Enter(key));
            for &child in &node.children {
                push(store, child, out);
            }
            out.push(Piece::Leave(key));
        }
        // A block-level box inside an inline formatting context is a box the tree builder should
        // have split the context around. Treating it as an atomic inline keeps it on the page and
        // sized rather than dropping it, which is the failure a reader could not see.
        _ => out.push(Piece::Atomic(key)),
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::{Piece, pieces};

    /// A store holding `root > span > text`, plus a sibling image after the span.
    #[test]
    fn a_nested_inline_box_is_entered_and_left_around_its_contents() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let style = StyleDraft::initial().build();
        let root = store.insert(BoxNode::new(
            style.clone(),
            BoxKind::AnonymousInlineRoot,
            FormattingContext::Inline,
        ));
        let span = store.insert(BoxNode::new(
            style.clone(),
            BoxKind::Element,
            FormattingContext::Inline,
        ));
        let text = store.insert(
            BoxNode::new(style.clone(), BoxKind::TextRun, FormattingContext::Inline)
                .with_text("hello"),
        );
        let image = store.insert(BoxNode::new(
            style,
            BoxKind::Element,
            FormattingContext::Replaced,
        ));
        store.get_mut(span).expect("live").children = vec![text];
        store.get_mut(root).expect("live").children = vec![span, image];

        assert_eq!(
            pieces(store.structure(), root),
            vec![
                Piece::Enter(span),
                Piece::Text(text),
                Piece::Leave(span),
                Piece::Atomic(image),
            ]
        );
    }
}
