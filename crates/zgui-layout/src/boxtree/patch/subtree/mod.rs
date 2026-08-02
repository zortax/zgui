//! Making one element's boxes again, and leaving every other box in the tree alone.
//!
//! A structural change is local: `display` moved on one element, one list gained a row, one panel
//! was mounted. What the document owes for it is the boxes those elements generate and nothing
//! else — but the obligation to rebuild propagates to the root, so a stage that asks *the root*
//! whether anything owes a rebuild is told "yes" for a change three levels deep inside one panel,
//! and replaces every box in the document. At the scale of a real application that is a tenth of a
//! second per wheel notch, spent renaming boxes that did not change.
//!
//! This is the other answer: ask *which* elements owe a rebuild, make their boxes again, and splice
//! them in where the old ones were.
//!
//! # What it refuses, and why refusing is the point
//!
//! A subtree can be spliced only when it is **confined**: when the boxes it holds are exactly the
//! boxes its own elements generate, when nothing outside it is laid out from inside it, and when
//! the box that goes in takes part in its container the same way the box that comes out did. Every
//! one of those is proved rather than assumed, and a subtree that fails any of them is handed back.
//!
//! Refusing costs a frame time. Splicing something that was not confined costs correctness: a dead
//! key in an ancestor's child list, a box positioned against a containing block that has been
//! removed, an inline run that should have been wrapped again and was not. So every case this
//! cannot prove is a case it declines.
//!
//! The largest thing it declines today is a box whose *layout* parent and *paint* parent are two
//! different boxes — an out-of-flow box positioned against something further up than the element it
//! was written inside. Replacing such a box means repairing two lists that are reached two
//! different ways, and only one of them is reachable from the box being replaced. It is a rebuild
//! from the root instead, which is correct and slower.

mod children;
mod confine;
mod place;
mod target;

use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex};

use crate::boxtree::build::{Owed, Subtree, blockify_item, build_subtree};
use crate::boxtree::patch::structure;
use crate::node::kind::FormattingContext;
use crate::style::convert::display::Participation;
use crate::tree::store::LayoutStore;

/// What one call to [`rebuild`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rebuilt {
    /// How many elements had their boxes made again.
    pub subtrees: u32,
    /// How many boxes were taken out of the tree.
    pub removed: u32,
}

/// Makes the boxes of everything `owed` names again, in place.
///
/// `owed` is what the invalidation walk found: the elements whose own boxes changed, and the
/// elements whose child list changed. The first are serviced by rebuilding their *container* —
/// wrapping, ordering and blockification are the container's decisions, and a child whose `display`
/// moved re-wraps siblings that carry no mark at all — and the second by rebuilding themselves,
/// because they already are the container.
///
/// Returns nothing when the change cannot be confined to the subtrees it names, which leaves the
/// caller to build the whole document's box tree. Some of those subtrees may already have been
/// spliced in when that happens; each splice is correct on its own, and the build that follows
/// replaces them along with everything else.
///
/// # Panics
///
/// Panics if `owed` names a node that is not live in `document`.
pub fn rebuild(store: &mut LayoutStore, document: &Document, owed: &Owed) -> Option<Rebuilt> {
    let targets = target::containers(document, owed)?;
    let mut done = Rebuilt::default();
    for target in targets {
        // A container whose child list moved is asked the narrower question first: make the boxes of
        // the children that moved, and keep the ones that did not. Rebuilding it is what happens when
        // that is refused — and it is refused for exactly the cases where the container would arrange
        // its remaining children differently, which is what makes keeping them safe.
        if target::gained_or_lost_a_child(owed, target) {
            let stale = target::stale_children(document, owed, target);
            if let Some(spliced) = children::splice(store, document, target, &stale) {
                done.removed += spliced.removed;
                done.subtrees += spliced.built;
                continue;
            }
        }
        done.removed += one(store, document, target)?;
        done.subtrees += 1;
    }
    Some(done)
}

/// Rebuilds one element's boxes, and reports how many boxes were taken out of the tree.
fn one(store: &mut LayoutStore, document: &Document, target: NodeIndex) -> Option<u32> {
    let old = place::primary_box(store, document, target)?;
    let held = confine::confined(store, document, target, old)?;
    let removed = u32::try_from(held.subtree.len()).unwrap_or(u32::MAX);
    let containing_block = place::containing_block(store, held.parent);
    let parent_style = document
        .store()
        .core(target)
        .parent()
        .and_then(|parent| document.node(parent).primary_style());

    let Some(built) = build_subtree(
        store,
        document,
        target,
        containing_block,
        parent_style.as_ref(),
    ) else {
        // The element generates no box at all any more, so the splice is a removal — and a removal
        // is only local in a container whose remaining children cannot be re-wrapped by it.
        if !confine::removable(store, held.parent) {
            return None;
        }
        return Some(structure::detach(store, old));
    };

    // The container's own treatment of an item it holds. The builder applies it from the
    // container's side, which is not being rebuilt, so it is applied here instead.
    if matches!(
        store.node(held.parent).fc,
        FormattingContext::Flex | FormattingContext::Grid
    ) {
        blockify_item(store, built.root.key);
    }

    if !takes_the_same_part(
        store,
        document,
        target,
        old,
        &built,
        &held,
        containing_block,
    ) {
        // Built and then thrown away rather than predicted: what a box becomes is decided by the
        // builder over the whole subtree, and a prediction computed here would be a second
        // implementation of those rules, agreeing with the first only until one of them changed.
        // The new boxes hang under nothing yet, so removing them removes nothing else.
        structure::detach(store, built.root.key);
        return None;
    }

    structure::replace(store, old, built.root.key);
    // The subtree's own root is left out: `replace` has just put it exactly where the box it stands
    // in for was, and attaching it again would give its containing block two entries for one box.
    crate::boxtree::absolute::attach_all(
        store,
        &built
            .reparented
            .iter()
            .copied()
            .filter(|item| item.key != built.root.key)
            .collect::<Vec<_>>(),
    );
    Some(removed)
}

/// Whether the box that goes in takes part in its container exactly as the box that came out did.
///
/// Three questions, and each is a way the container's own arrangement would change under a box that
/// is not being rebuilt with it.
///
/// **Is it in the same list?** A box that is out of flow is laid out by the ancestor that positions
/// it and painted where it was written, and one that is in flow is laid out by the container it was
/// written in. The old box hangs under exactly one box in both orders — [`confine::confined`] made
/// sure of that — so the new box has to want to hang under that same one: its containing block if
/// it is out of flow, and the box its own element's parent generated if it is not.
///
/// **Is it the same kind of participant?** A box that is block-level where the old one was
/// inline-level changes how its *siblings* are wrapped into anonymous inline boxes, and its
/// siblings carry no mark.
///
/// **Is it the same kind of box?** A formatting context or a box kind that moved is a container
/// laying its own children out by different rules, which the boxes above it were sized against.
fn takes_the_same_part(
    store: &LayoutStore,
    document: &Document,
    target: NodeIndex,
    old: BoxKey,
    built: &Subtree,
    held: &confine::Held,
    containing_block: Option<BoxKey>,
) -> bool {
    if built.root.participation == Participation::Contents {
        return false;
    }
    let hangs_right = if built.root.out_of_flow {
        containing_block == Some(held.parent)
    } else {
        written_under(store, document, target, held.parent)
    };
    if !hangs_right {
        return false;
    }
    let (Some(old), Some(new)) = (store.get(old), store.get(built.root.key)) else {
        return false;
    };
    old.block_level == new.block_level && old.fc == new.fc && old.kind == new.kind
}

/// Whether `parent` is the box an in-flow child of `target`'s element would be written into.
///
/// Either the box the element's own parent generated, or an anonymous box that container made to
/// hold a run of inline-level children — which is one level down and belongs to that container.
fn written_under(
    store: &LayoutStore,
    document: &Document,
    target: NodeIndex,
    parent: BoxKey,
) -> bool {
    let Some(index) = document.store().core(target).parent() else {
        return false;
    };
    let Some(writer) = place::primary_box(store, document, index) else {
        return false;
    };
    parent == writer
        || store
            .get(parent)
            .is_some_and(|node| node.source.is_none() && node.parent == Some(writer))
}
