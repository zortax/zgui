//! Proving that a container's child list can be serviced child by child, and working out how.
//!
//! Everything here is a refusal or a step. The refusals are what make the splice safe to commit:
//! each one names a way the container's *own* arrangement of its children would change under a child
//! that is put in or taken out beside boxes nothing rebuilt.

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_css::ComputedStyle;
use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex, NodeKind};

use crate::boxtree::classify::classify;
use crate::boxtree::patch::subtree::{confine, place};
use crate::node::kind::FormattingContext;
use crate::style::convert::display::Participation;
use crate::tree::store::LayoutStore;

/// What happens to one position in the container's child list.
#[derive(Clone, Copy, Debug)]
pub(super) enum Step {
    /// The child keeps the boxes it has, under the name it has.
    Keep(BoxKey),
    /// The child's boxes are made, because it has none or because what it had has to go.
    Build(NodeIndex),
}

/// A proved plan for one container's child list.
pub(super) struct Plan {
    /// The container's own box, whose two child lists are rewritten.
    pub(super) container: BoxKey,
    /// What the container lays its children out by.
    pub(super) fc: FormattingContext,
    /// The box that positions an out-of-flow box written inside the container.
    pub(super) containing_block: Option<BoxKey>,
    /// The style an anonymous box directly inside the container inherits from.
    pub(super) style: ComputedStyle,
    /// Every position of the new child list, in document order.
    pub(super) steps: Vec<Step>,
    /// The boxes that go, each the root of a subtree proved to hang in exactly one place.
    pub(super) departing: Vec<BoxKey>,
}

/// Works out how to service `container`'s child list, or refuses.
///
/// `stale` names the children whose own subtrees owe a rebuild, which are built again rather than
/// kept: an obligation marked below one of them is serviced by making that child's boxes, and a plan
/// that kept it would leave the obligation unserviced with nothing left to report it.
///
/// # What is refused, and why each refusal is a way the tree would be wrong
///
/// **A container that holds a run of inline-level boxes.** A child appearing or disappearing inside
/// one re-breaks the run into different anonymous boxes, and those boxes belong to the container.
/// Every child having a box of its own, in both orders, in the same sequence, is what proves there is
/// no run: an anonymous wrapper appears in the layout order and not in the paint one.
///
/// **A box the container did not generate for one of its own children.** A `::before`, a `::after`
/// and a list item's mark all name the container as their source and all sit among its children, so
/// a plan built by matching child boxes to child elements would take one of them for a child that
/// has left.
///
/// **`order` on a flex or grid item.** The property is applied by sorting the layout child list, so a
/// container holding one item that carries it lays its children out in an order the document does not
/// give — and where the arriving child belongs in that order is a question only a full sort answers.
///
/// **A departing subtree that does not hang in one place.** An out-of-flow box positioned into a
/// subtree from outside is laid out by an ancestor and painted where it was written, so destroying
/// the subtree it was positioned into leaves a name behind in a list nothing below it can reach.
///
/// **A child whose own children take its place in the tree.** `display: contents` puts a child's
/// descendants into this container's child list, so one child is any number of boxes there — and the
/// one child, one box correspondence is what every step here is matched by.
///
/// **A child that already has boxes the plan is not taking out.** Building it again would leave the
/// element owning two sets of boxes, and every question answered by unioning an element's pieces
/// would be answered with the union of where it is and where it used to be.
pub(super) fn of(
    store: &LayoutStore,
    document: &Document,
    container: NodeIndex,
    stale: &FxHashSet<NodeIndex>,
) -> Option<Plan> {
    let own = place::primary_box(store, document, container)?;
    let node = store.get(own)?;
    if node.fc == FormattingContext::Inline {
        return None;
    }
    // Both orders, as one sequence. A container whose two lists differ holds an anonymous wrapper, an
    // out-of-flow child or a sorted `order`, and each of those is the container's own arrangement
    // rather than a child's.
    if node.children != node.paint_children {
        return None;
    }
    let fc = node.fc;
    let held = node.children.clone();

    let mut kept: FxHashMap<NodeIndex, BoxKey> = FxHashMap::default();
    let mut departing = Vec::new();
    let mut doomed: FxHashSet<BoxKey> = FxHashSet::default();
    let own_key = document.store().key_of(container);
    for key in held {
        let child = store.get(key)?;
        if child.pseudo.is_some() || !child.block_level {
            return None;
        }
        if matches!(fc, FormattingContext::Flex | FormattingContext::Grid)
            && child.style.get_position().order != 0
        {
            return None;
        }
        let source = child.source?;
        if source == own_key {
            return None;
        }
        let index = document.store().index_of(source)?;
        let mine = document.store().core(index).parent() == Some(container);
        if mine && !stale.contains(&index) {
            kept.insert(index, key);
            continue;
        }
        // The box goes, so what it takes with it has to be exactly its own subtree. Which proof
        // establishes that depends on whether the child is still in the document: one whose boxes are
        // merely stale still has its ancestry, and one that has left has none left to read.
        let confined = if mine {
            confine::confined(store, document, index, key)?
        } else {
            confine::departed(store, document, key)?
        };
        doomed.extend(confined.subtree.iter().copied());
        departing.push(key);
    }

    let mut steps = Vec::new();
    let mut next = document.store().core(container).first_child();
    while let Some(child) = next {
        next = document.store().core(child).next_sibling();
        match document.store().core(child).kind() {
            NodeKind::Element => {}
            // A node that holds no position among element siblings generates no box either.
            NodeKind::Marker => continue,
            // A run of text is inline-level, and how a container wraps one is the container's own
            // decision rather than the run's.
            _ => return None,
        }
        // A child whose own children take its place in the tree contributes as many boxes to this
        // container as it has descendants that generate one, and it contributes none of its own. The
        // one-child-one-box correspondence every step below rests on does not hold for it.
        if document
            .node(child)
            .primary_style()
            .is_some_and(|style| classify(&style).participation == Participation::Contents)
        {
            return None;
        }
        if let Some(&key) = kept.get(&child) {
            steps.push(Step::Keep(key));
            continue;
        }
        if !store
            .boxes_of(document.store().key_of(child))
            .iter()
            .all(|key| doomed.contains(key))
        {
            return None;
        }
        steps.push(Step::Build(child));
    }

    Some(Plan {
        container: own,
        fc,
        containing_block: place::containing_block(store, own),
        style: document.node(container).primary_style()?,
        steps,
        departing,
    })
}
