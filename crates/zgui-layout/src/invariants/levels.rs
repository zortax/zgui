//! What must be true of the box tree, the fragment tree and the index over them.

use zgui_dom::side::BoxKey;

use crate::fragment::hit::HitIndex;
use crate::tree::store::LayoutStore;

/// One way the three levels disagreed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// What was found to be wrong, in a form a person can act on.
    pub what: String,
}

impl Violation {
    /// Records one violation.
    fn new(what: impl Into<String>) -> Self {
        Self { what: what.into() }
    }
}

/// Reports whether the checks are switched on outside tests.
///
/// The checks are on when `ZGUI_INVARIANTS` is set to a value other than empty or `0`. The
/// variable is read once per process.
pub fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("ZGUI_INVARIANTS").is_ok_and(|value| value != "0" && !value.is_empty())
    })
}

/// Runs the checks if they are switched on, and panics on the first violation.
///
/// The panic is the point: a violation means a later stage is about to read a structure that is not
/// what it claims to be, and the failure it produces there says nothing about the cause.
///
/// # Panics
///
/// If any of the three levels disagree.
pub fn check_if_enabled(store: &LayoutStore, hit: &HitIndex) {
    if !enabled() {
        return;
    }
    let violations = check(store, hit);
    assert!(violations.is_empty(), "layout invariants: {violations:?}");
}

/// Every way the three levels disagree, which is empty for a tree that is intact.
pub fn check(store: &LayoutStore, hit: &HitIndex) -> Vec<Violation> {
    let mut out = Vec::new();
    let Some(root) = store.root() else {
        return out;
    };
    // Everything painted is laid out. Not the converse: an anonymous wrapper is a box the layout
    // algorithms need and the document never mentions, so it is in its parent's layout children and
    // in nobody's document order. A box painted without being laid out has no position at all, and
    // the symptom is a rectangle drawn where the last frame left it.
    let laid_out = reachable(store, root, |node| &node.children);
    let painted = reachable(store, root, |node| &node.paint_children);
    for key in &painted {
        if !laid_out.contains(key) {
            out.push(Violation::new(format!(
                "box {} is painted but never laid out",
                key.index()
            )));
        }
    }
    let mut all: Vec<BoxKey> = laid_out.iter().copied().collect();
    all.extend(
        painted
            .iter()
            .copied()
            .filter(|key| !laid_out.contains(key)),
    );
    all.sort_unstable();
    all.dedup();
    for key in all {
        check_box(store, key, &mut out);
        check_rosters(store, key, &mut out);
        check_fragments(store, hit, key, &mut out);
    }
    check_root(store, root, &mut out);
    // The damage expansion walks the registry on every frame, so an entry that is listed twice or
    // names a removed fragment makes every later frame slower.
    let mut listed = std::collections::BTreeSet::new();
    for frag in store.read_extents() {
        if !listed.insert(*frag) {
            out.push(Violation::new(format!(
                "the read-extent registry lists fragment {} more than once",
                frag.index()
            )));
        }
        if store.fragment(*frag).is_none() {
            out.push(Violation::new(format!(
                "the read-extent registry lists fragment {}, which no longer exists",
                frag.index()
            )));
        }
    }
    // A fragment that ceased to exist and left its entry behind answers hits for ever, at the place
    // the deleted content used to be and in front of whatever now occupies it. Nothing else notices:
    // the index stays internally consistent, and the fragment tree has no idea it is being named.
    for frag in hit.indexed() {
        if store.fragment(frag).is_none() {
            out.push(Violation::new(format!(
                "the hit index holds an entry for fragment {}, which no longer exists",
                frag.index()
            )));
        }
    }
    if !hit.is_consistent() {
        out.push(Violation::new(
            "the hit index and its spatial hierarchy hold different numbers of entries",
        ));
    }
    out
}

/// Every box reachable from `root` by following one of the two child orders.
fn reachable(
    store: &LayoutStore,
    root: BoxKey,
    children: impl Fn(&crate::node::box_node::BoxNode) -> &Vec<BoxKey>,
) -> std::collections::BTreeSet<BoxKey> {
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        if let Some(node) = store.get(key) {
            stack.extend(children(node).iter().copied());
        }
    }
    seen
}

/// Every box is listed by the box it names as its parent, and by the element it came from.
fn check_box(store: &LayoutStore, key: BoxKey, out: &mut Vec<Violation>) {
    let Some(node) = store.get(key) else {
        out.push(Violation::new(format!(
            "box {} does not exist",
            key.index()
        )));
        return;
    };
    if let Some(parent) = node.parent {
        match store.get(parent) {
            None => out.push(Violation::new(format!(
                "box {} names a parent that does not exist",
                key.index()
            ))),
            Some(record)
                if !record.children.contains(&key) && !record.paint_children.contains(&key) =>
            {
                out.push(Violation::new(format!(
                    "box {} is not among its parent's children",
                    key.index()
                )))
            }
            Some(_) => {}
        }
    }
    if let Some(source) = node.source
        && !store.boxes_of(source).contains(&key)
    {
        out.push(Violation::new(format!(
            "box {} names an element that does not list it",
            key.index()
        )));
    }
    // The interned entry is where per-style derivations live, so a slot naming a different
    // allocation than the box holds serves derivations of a style the box no longer has. The
    // symptom appears far away: a box laid out with margins from a style it wore frames ago.
    match store.interned_style(key) {
        None => out.push(Violation::new(format!(
            "box {} holds no interned style slot",
            key.index()
        ))),
        Some(interned) if !crate::style::same_cascade(interned, &node.style) => {
            out.push(Violation::new(format!(
                "box {}'s interned style slot names a different cascade result than the box holds",
                key.index()
            )));
        }
        Some(_) => {}
    }
}

/// Every box's roster memberships say what its style says.
///
/// The rosters are maintained rather than recomputed, so they are correct exactly as long as every
/// place that establishes or replaces a box's style goes through
/// [`LayoutStore::set_style`](crate::tree::store::LayoutStore::set_style). This is what notices the
/// day a fourth one is added: a stale membership bit is not a crash, it is a `fit-content` box that
/// is silently never measured, or a scrollport whose gutter is never revised, and neither has a
/// symptom at the point of the mistake.
fn check_rosters(store: &LayoutStore, key: BoxKey, out: &mut Vec<Violation>) {
    let Some(node) = store.get(key) else {
        return;
    };
    let content = crate::intrinsic::keywords::axes_of(&node.style);
    if content != store.content_axes(key) {
        out.push(Violation::new(format!(
            "box {} is on the content-keyword roster as {:?} and its style says {content:?}",
            key.index(),
            store.content_axes(key)
        )));
    }
    let overflow = crate::style::convert::overflow::undecided_axes(&node.style);
    if overflow != store.undecided_overflow(key) {
        out.push(Violation::new(format!(
            "box {} is on the undecided-overflow roster as {:?} and its style says {overflow:?}",
            key.index(),
            store.undecided_overflow(key)
        )));
    }
}

/// Every fragment names a live box, is listed by it, and is listed by its element.
fn check_fragments(store: &LayoutStore, hit: &HitIndex, key: BoxKey, out: &mut Vec<Violation>) {
    for &frag in store.fragments_of_box(key) {
        let Some(fragment) = store.fragment(frag) else {
            out.push(Violation::new(format!(
                "box {} lists a fragment that does not exist",
                key.index()
            )));
            continue;
        };
        if fragment.box_ != key {
            out.push(Violation::new(format!(
                "fragment {} is listed by box {} and names box {}",
                frag.index(),
                key.index(),
                fragment.box_.index()
            )));
        }
        if let Some(node) = fragment.node
            && !store.fragments_of(node).contains(&frag)
        {
            out.push(Violation::new(format!(
                "fragment {} names an element that does not list it",
                frag.index()
            )));
        }
        if !holds(fragment.subtree_ink, fragment.ink) && !fragment.ink.is_empty() {
            out.push(Violation::new(format!(
                "fragment {}'s subtree ink does not contain its own ink",
                frag.index()
            )));
        }
        if fragment
            .flags
            .contains(crate::fragment::FragmentFlags::HAS_READ_EXTENT)
            != store.read_extents().contains(&frag)
        {
            out.push(Violation::new(format!(
                "fragment {} disagrees with the read-extent registry about its own membership",
                frag.index()
            )));
        }
        if let Some(entry) = hit.entry(frag)
            && entry.frag != frag
        {
            out.push(Violation::new(format!(
                "the hit index holds an entry for fragment {} naming fragment {}",
                frag.index(),
                entry.frag.index()
            )));
        }
    }
}

/// The root box has no parent, and every other box is below it.
fn check_root(store: &LayoutStore, root: BoxKey, out: &mut Vec<Violation>) {
    if store.get(root).and_then(|node| node.parent).is_some() {
        out.push(Violation::new("the root box has a parent"));
    }
}

/// Reports whether `outer` holds `inner`, edges that differ by float rounding included.
///
/// A rectangle stores its origin and size, so a union's far edge is a difference added back to the
/// origin, which can land a rounding step short of the edge it was taken from.
fn holds(
    outer: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
    inner: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
) -> bool {
    const ROUNDING: f32 = 1.0 / 256.0;
    let left = outer.origin.x.0 - ROUNDING;
    let top = outer.origin.y.0 - ROUNDING;
    let right = outer.origin.x.0 + outer.size.width.0 + ROUNDING;
    let bottom = outer.origin.y.0 + outer.size.height.0 + ROUNDING;
    inner.origin.x.0 >= left
        && inner.origin.y.0 >= top
        && inner.origin.x.0 + inner.size.width.0 <= right
        && inner.origin.y.0 + inner.size.height.0 <= bottom
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;

    use crate::fragment::hit::HitIndex;
    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::check;

    #[test]
    fn a_store_with_no_root_has_nothing_to_disagree_about() {
        let store = LayoutStore::new(DocumentId::FIRST);
        assert!(check(&store, &HitIndex::new()).is_empty());
    }

    #[test]
    fn a_box_whose_parent_disowns_it_is_reported() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let root = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        let child = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        store.set_root(root);
        store.get_mut(root).expect("live").children.push(child);
        store
            .get_mut(root)
            .expect("live")
            .paint_children
            .push(child);
        store.get_mut(child).expect("live").parent = Some(root);
        assert!(check(&store, &HitIndex::new()).is_empty());

        // The parent stops laying the child out while still painting it, which is the shape of
        // every "it is on the screen and nothing can click it" bug this checker exists for.
        store.get_mut(root).expect("live").children.clear();
        let violations = check(&store, &HitIndex::new());
        assert!(
            violations
                .iter()
                .any(|violation| violation.what.contains("never laid out")),
            "{violations:?}"
        );
    }
}
