//! Proving that one element's boxes are the only boxes a splice touches.

use rustc_hash::FxHashSet;
use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex};

use crate::boxtree::patch::subtree::target;
use crate::node::kind::{BoxKind, FormattingContext};
use crate::tree::store::LayoutStore;

/// What a confined subtree is, once it has been proved to be one.
pub(super) struct Held {
    /// The box the subtree's root hangs under, in both child orders.
    pub(super) parent: BoxKey,
    /// Every box in the subtree, including its root.
    pub(super) subtree: Vec<BoxKey>,
}

/// Proves that the boxes below `old` are exactly the boxes `target` and its descendants generate.
///
/// Three things are checked, and each is a way a splice would otherwise corrupt the tree.
///
/// **The box hangs in one place.** It has to be a child of one box in *both* the layout order and
/// the paint order. A box that is in one and not the other is out of flow — laid out by an ancestor
/// and painted where it was written — and replacing it leaves a dead key in whichever list was not
/// looked at.
///
/// **Nothing inside is laid out from outside.** Every box below the root must have its layout
/// parent inside the subtree, or something out there is holding a name that is about to be removed.
///
/// **Nothing outside is written inside.** Every box in the subtree that names an element must name
/// `target` or an element below it, or an absolutely positioned box written somewhere else has been
/// positioned into this subtree and is about to be destroyed with it.
pub(super) fn confined(
    store: &LayoutStore,
    document: &Document,
    target: NodeIndex,
    old: BoxKey,
) -> Option<Held> {
    proved(store, document, old, &|index| {
        target::is_at_or_below(document, index, target)
    })
}

/// The same proof for the boxes of a subtree that has **left** the document.
///
/// What the third question above asks — is every element named inside this subtree at or below its
/// root — cannot be asked of a subtree that has been removed, and the reason is not a technicality. A
/// view unmounts by taking its nodes out one at a time from the inside, so a removed row is a row whose
/// own children were unlinked from it before it was unlinked from its parent: nothing in it has a
/// parent, ancestry says nothing about any of it, and every box in it reads as written somewhere else.
///
/// So the question is put the other way round. Every element named inside a departed subtree must
/// itself have left the document, whatever is left of the links between them. That refuses exactly what
/// the ancestry test refuses — a box written somewhere the document can still reach, positioned into
/// this subtree, whose paint order is a list nothing here can repair — while admitting the subtree the
/// caller is actually taking out.
pub(super) fn departed(store: &LayoutStore, document: &Document, root: BoxKey) -> Option<Held> {
    proved(store, document, root, &|index| gone(document, index))
}

/// Whether `index` can no longer be reached from the document node.
///
/// A node still in the document reaches it through its parents, the root element's parent being the
/// document node itself. So a walk that runs out of parents anywhere else is a walk over something that
/// has been taken out — which is the state every node of a removed subtree is in, whether they were
/// unlinked one at a time or left linked under a removed root.
fn gone(document: &Document, index: NodeIndex) -> bool {
    let store = document.store();
    let end = document.document_index();
    let mut next = Some(index);
    while let Some(current) = next {
        if current == end {
            return false;
        }
        next = store.core(current).parent();
    }
    true
}

/// The three questions, with `written_inside` deciding which elements may be named inside the subtree.
fn proved(
    store: &LayoutStore,
    document: &Document,
    old: BoxKey,
    written_inside: &dyn Fn(NodeIndex) -> bool,
) -> Option<Held> {
    let parent = store.get(old)?.parent?;
    let above = store.get(parent)?;
    if !above.children.contains(&old) || !above.paint_children.contains(&old) {
        return None;
    }

    let subtree = collect(store, old);
    let inside: FxHashSet<BoxKey> = subtree.iter().copied().collect();
    for &key in &subtree {
        let node = store.get(key)?;
        if key != old && !node.parent.is_some_and(|parent| inside.contains(&parent)) {
            return None;
        }
        if let Some(source) = node.source {
            let index = document.store().index_of(source)?;
            if !written_inside(index) {
                return None;
            }
        }
    }
    Some(Held { parent, subtree })
}

/// Every box at or below `root`, in both child orders, each named once.
fn collect(store: &LayoutStore, root: BoxKey) -> Vec<BoxKey> {
    let mut seen: FxHashSet<BoxKey> = FxHashSet::default();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        out.push(key);
        let Some(node) = store.get(key) else {
            continue;
        };
        stack.extend(node.children.iter().copied());
        stack.extend(node.paint_children.iter().copied());
    }
    out
}

/// Whether taking one box out of `parent` can change nothing about the boxes left in it.
///
/// It can, in general: removing a block-level box between two inline runs merges them into one
/// anonymous inline box, and removing the only inline child takes an anonymous box away. So a
/// removal is spliced only into a container that holds no inline run at all — every child
/// block-level, no anonymous wrapper — where the boxes that remain are the boxes that remained.
pub(super) fn removable(store: &LayoutStore, parent: BoxKey) -> bool {
    let Some(node) = store.get(parent) else {
        return false;
    };
    if node.fc == FormattingContext::Inline {
        return false;
    }
    node.children.iter().all(|&child| {
        store
            .get(child)
            .is_some_and(|child| child.kind != BoxKind::AnonymousInlineRoot && child.block_level)
    })
}
