//! Which elements have to have their boxes made again.

use rustc_hash::FxHashSet;
use zgui_dom::{Document, NodeIndex, NodeKind};

use crate::boxtree::build::Owed;

/// How many *containers* are worth splicing one at a time.
///
/// Past this the marks are no longer a local change: every splice pays for its own confinement
/// test, and building the whole tree once is both simpler and cheaper. The number is a threshold
/// and not a boundary of correctness — either path is correct at any count.
///
/// Counted over the collapsed, outermost containers rather than over the raw marks, because that
/// is what the splices scale with: a virtualised list crossing a dozen row boundaries in one fast
/// frame marks hundreds of elements, and every one of them collapses to the list's own pane — one
/// container, one child-by-child splice, however many rows arrived.
const MOST: usize = 64;

/// How many raw marks are worth collapsing at all.
///
/// The collapse itself is a hash insert per mark, so it is cheap — but a frame that marked half
/// the document is not a local change whatever it collapses to, and the whole-tree build answers
/// it without the collection pass.
const RAW_MOST: usize = 4_096;

/// The elements whose boxes are rebuilt to service `owed`, outermost only.
///
/// An element whose **own boxes** changed is serviced by rebuilding its *container*, not itself.
/// Which boxes a child generates is decided by the child, but how they are wrapped into anonymous
/// inline boxes, what order they are laid out in and whether they are blockified are all decided by
/// the container — so a child whose `display` moved re-wraps siblings that carry no mark at all.
///
/// An element whose **child list** changed is already the container, and is serviced by rebuilding
/// itself. Going up another level from there would rebuild a whole page to service one inserted
/// row.
///
/// A container that is inside another container in the same set is dropped: rebuilding the outer
/// one makes the inner one's boxes again anyway, and splicing the inner one afterwards would splice
/// it into a tree that no longer holds the box it was measured against.
///
/// `None` when the change is not local — when something marked has no container with boxes of its
/// own, which the root element is, or when there are more marks than a splice-by-splice pass is
/// worth.
pub(super) fn containers(document: &Document, owed: &Owed) -> Option<Vec<NodeIndex>> {
    if owed.is_empty() || owed.len() > RAW_MOST {
        return None;
    }
    let store = document.store();
    let mut set: FxHashSet<NodeIndex> = FxHashSet::default();
    for &node in &owed.rebuilt {
        let parent = store.core(node).parent()?;
        if store.core(parent).kind() != NodeKind::Element {
            return None;
        }
        set.insert(parent);
    }
    for &node in &owed.children {
        if store.core(node).kind() != NodeKind::Element {
            return None;
        }
        set.insert(node);
    }
    let outermost: Vec<NodeIndex> = set
        .iter()
        .copied()
        .filter(|&node| !has_ancestor_in(document, node, &set))
        .collect();
    if outermost.len() > MOST {
        return None;
    }
    Some(outermost)
}

/// Whether `container`'s own child list is among what it owes.
///
/// The question a child-by-child splice can be asked at all. A container reached only through
/// [`Owed::rebuilt`] is the container of a child whose own boxes changed and nothing else has moved
/// in it, which is what the whole-subtree splice above already services in one pass; a container that
/// gained or lost a child is where rebuilding it means rebuilding every child it kept.
pub(super) fn gained_or_lost_a_child(owed: &Owed, container: NodeIndex) -> bool {
    owed.children.contains(&container)
}

/// The children of `container` whose own subtrees hold something that owes a rebuild.
///
/// An obligation marked below one of them is serviced by making that child's boxes again, so a splice
/// that kept the child would leave the obligation unserviced — and it has already been retired, so
/// nothing would ever report it again. `container`'s own marks are not in the answer: they are what
/// the splice itself services.
pub(super) fn stale_children(
    document: &Document,
    owed: &Owed,
    container: NodeIndex,
) -> FxHashSet<NodeIndex> {
    let store = document.store();
    let mut set = FxHashSet::default();
    for &node in owed.rebuilt.iter().chain(owed.children.iter()) {
        let mut below = node;
        let mut next = store.core(node).parent();
        while let Some(current) = next {
            if current == container {
                set.insert(below);
                break;
            }
            below = current;
            next = store.core(current).parent();
        }
    }
    set
}

/// Whether any ancestor of `node` is in `set`.
fn has_ancestor_in(document: &Document, node: NodeIndex, set: &FxHashSet<NodeIndex>) -> bool {
    let store = document.store();
    let mut next = store.core(node).parent();
    while let Some(current) = next {
        if set.contains(&current) {
            return true;
        }
        next = store.core(current).parent();
    }
    false
}

/// Whether `node` is `ancestor` or is below it.
pub(super) fn is_at_or_below(document: &Document, node: NodeIndex, ancestor: NodeIndex) -> bool {
    let store = document.store();
    let mut next = Some(node);
    while let Some(current) = next {
        if current == ancestor {
            return true;
        }
        next = store.core(current).parent();
    }
    false
}
