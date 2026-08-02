//! Servicing a container's child list without rebuilding the children that stayed.
//!
//! One element's boxes can be spliced into a tree that is already there — that is what the module
//! above this one does — and for a change *inside* one child of a long list that is the whole answer.
//! It is not the answer for a change to the list itself. A container that gained a child owes the
//! boxes of that child, and rebuilding the container makes the boxes of every child it has: a
//! virtualised list scrolled by one row is a container of thirty rows that gained one and lost one,
//! and rebuilding it renames all thirty. Every fragment in the port then compares as changed, none of
//! their paint records can be reused, and the damage grows to the port's own ink.
//!
//! So this is the narrower operation: keep the children that are still there, make the boxes of the
//! ones that are not, take out the ones that have left, and relink the container's two child lists in
//! document order.
//!
//! # Why keeping a child is a claim about the container
//!
//! Which boxes a child generates is the child's own decision, but how they are wrapped into anonymous
//! inline boxes, what order they are laid out in and whether they are blockified are all the
//! container's. A child put in beside boxes nothing rebuilt is therefore only safe where the
//! container makes none of those decisions differently for having one more child — and that is proved
//! rather than assumed. [`plan`] holds each proof and what it refuses; a container that fails any of
//! them is handed back, and the caller rebuilds its whole subtree instead.

mod plan;

use rustc_hash::FxHashSet;
use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex};

use crate::boxtree::absolute::{Reparented, attach_all};
use crate::boxtree::build::{blockify_item, build_subtree};
use crate::boxtree::patch::structure;
use crate::boxtree::patch::subtree::children::plan::{Plan, Step};
use crate::node::kind::FormattingContext;
use crate::tree::dirty::mark_dirty;
use crate::tree::store::LayoutStore;

/// What one call to [`splice`] did.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Spliced {
    /// How many children had their boxes made.
    pub(super) built: u32,
    /// How many boxes were taken out of the tree.
    pub(super) removed: u32,
}

/// Puts the children `container` has now into the boxes it has now, and reports what that cost.
///
/// `stale` names the children that must have their boxes made again whether or not they are new,
/// which is how an obligation marked below one of them is serviced.
///
/// Returns nothing, having changed nothing, when the change cannot be confined to the children that
/// moved — which leaves the caller to rebuild the container's whole subtree, or the document's.
///
/// # Panics
///
/// Panics if `container` names a node that is not live in `document`.
pub(super) fn splice(
    store: &mut LayoutStore,
    document: &Document,
    container: NodeIndex,
    stale: &FxHashSet<NodeIndex>,
) -> Option<Spliced> {
    let plan = plan::of(store, document, container, stale)?;
    // Everything is built before anything is taken out, so that a refusal is a refusal: a plan that
    // removed first and then found a child it could not put in would have left the tree neither the
    // old one nor the new one, and the build that follows a refusal starts from whatever is there.
    let made = build(store, document, &plan)?;

    let mut removed = 0;
    for key in plan.departing {
        removed += structure::detach(store, key);
    }
    for &key in &made.order {
        if let Some(node) = store.get_mut(key) {
            node.parent = Some(plan.container);
            node.parent_fc = plan.fc;
        }
    }
    let node = store.get_mut(plan.container)?;
    node.children = made.order.clone();
    node.paint_children = made.order;
    attach_all(store, &made.reparented);
    // A container whose child list moved is a container whose own size was computed from the children
    // it used to have, and so was every ancestor's.
    mark_dirty(store, plan.container);
    Some(Spliced {
        built: made.built,
        removed,
    })
}

/// The boxes a plan's [`Step::Build`] steps produced, and the child list they belong in.
struct Made {
    /// The container's new child list, in document order, for both of its orders.
    order: Vec<BoxKey>,
    /// The out-of-flow boxes below the children that were built, each with the box that positions it.
    reparented: Vec<Reparented>,
    /// How many children had their boxes made.
    built: u32,
}

/// Makes the boxes each [`Step::Build`] asks for, or refuses and leaves the tree as it was.
///
/// The refusal is *after* the build rather than predicted before it, for the same reason the splice
/// above works that way: what a box becomes is decided by the builder over the whole subtree, and a
/// prediction computed here would be a second implementation of those rules that agreed with the
/// first only until one of them changed. Boxes built and then thrown away hang under nothing, so
/// removing them removes nothing else.
fn build(store: &mut LayoutStore, document: &Document, plan: &Plan) -> Option<Made> {
    let mut made = Made {
        order: Vec::with_capacity(plan.steps.len()),
        reparented: Vec::new(),
        built: 0,
    };
    for &step in &plan.steps {
        let index = match step {
            Step::Keep(key) => {
                made.order.push(key);
                continue;
            }
            Step::Build(index) => index,
        };
        let Some(built) = build_subtree(
            store,
            document,
            index,
            plan.containing_block,
            Some(&plan.style),
        ) else {
            // The child generates no box at all, which `display: none` does. There is nothing to put
            // in the list, and taking a box out of a container whose children are all block-level
            // cannot re-wrap the ones that are left.
            continue;
        };
        made.built += 1;
        // The container's own treatment of an item it holds. The builder applies it from the
        // container's side, which is not being rebuilt, so it is applied here instead.
        if matches!(plan.fc, FormattingContext::Flex | FormattingContext::Grid) {
            blockify_item(store, built.root.key);
        }
        if !takes_its_place(store, built.root.key, built.root.out_of_flow, plan.fc) {
            discard(store, &made, built.root.key);
            return None;
        }
        made.reparented.extend(
            built
                .reparented
                .iter()
                .copied()
                .filter(|item| item.key != built.root.key),
        );
        made.order.push(built.root.key);
    }
    Some(made)
}

/// Whether a freshly built child takes part in its container the way every other child does.
///
/// The plan proved that the container holds nothing but block-level in-flow children with a box
/// each, which is what makes keeping the others safe. A child that arrives inline-level breaks that:
/// its siblings would be swept into an anonymous inline box, and they carry no mark. A child that
/// arrives out of flow breaks it too — it is laid out by an ancestor and painted where it was
/// written, so it belongs in one of the container's lists and not the other.
fn takes_its_place(
    store: &LayoutStore,
    key: BoxKey,
    out_of_flow: bool,
    fc: FormattingContext,
) -> bool {
    if out_of_flow {
        return false;
    }
    let Some(node) = store.get(key) else {
        return false;
    };
    if matches!(fc, FormattingContext::Flex | FormattingContext::Grid)
        && node.style.get_position().order != 0
    {
        return false;
    }
    node.block_level
}

/// Throws away every box a refused plan built, leaving the tree as it was.
fn discard(store: &mut LayoutStore, made: &Made, last: BoxKey) {
    structure::detach(store, last);
    for &key in &made.order {
        // Only the boxes this call made hang under nothing; a kept child is still in the tree, and
        // its parent still names it.
        if store.get(key).is_some_and(|node| node.parent.is_none()) {
            structure::detach(store, key);
        }
    }
}
