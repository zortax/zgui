//! Putting an element's new computed style onto the boxes it already generated.
//!
//! A box holds a clone of the style it was built with, and every stage after layout reads the
//! box's copy rather than the element's: the painter lowers `node.style` into backgrounds, borders,
//! shadows and outlines, and the layout algorithms read it for every length they resolve. A cascade
//! result is a fresh allocation each time it is computed, so an element restyled while its box is
//! kept leaves that box holding the *previous* cascade — and the frame then damages exactly the
//! right rectangle and repaints it in the colour that was already there.
//!
//! That failure is invisible to a damage assertion, because the damage is correct. It is what this
//! module exists to prevent, and it is why the copy is refreshed on the frame that restyled rather
//! than left to the next rebuild.
//!
//! # The boxes an element owns the style of
//!
//! More than one, and they are not all styled the same way. The element's own box takes the
//! element's style. A `::before` or `::after` box takes the pseudo-element's. A list item's mark,
//! the anonymous wrappers CSS requires around mixed runs, and the runs of text between an element's
//! children all take a *synthesised* style — inherited properties from the owner, everything else
//! at its initial value — because an anonymous box that inherited borders, padding or a background
//! would paint them a second time.
//!
//! So the refresh descends from each style-owning box through the boxes that have no style of their
//! own, and stops at the first box that has one. That box is an element in its own right; if its
//! cascade moved, it is in the caller's list too.

use zgui_bits::Dirty;
use zgui_css::ComputedStyle;
use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex, NodeKind};

use crate::boxtree::anonymous::synthesised_style;
use crate::node::box_node::BoxNode;
use crate::node::kind::{BoxKind, PseudoKind};
use crate::tree::dirty::mark_dirty;
use crate::tree::store::LayoutStore;

/// Refreshes the styles of every box `nodes` generated, and reports how many boxes moved.
///
/// Only boxes that are already in the tree are touched: this changes what a box is made of and
/// never which boxes exist, so an element whose `display` moved is not something it can service and
/// is not something it is asked to — such a change carries an obligation to rebuild, which the
/// caller honours first.
///
/// A box whose style is already the one the element now has is left alone, and reported as
/// unmoved. That is the common case for the descendants of a restyled subtree, where the cascade
/// re-ran and produced the allocation the box is already holding.
///
/// # Why a rewritten box may also have its layout thrown away
///
/// Because a style is not only what a box is painted with, it is what the box is *measured* with.
/// An element that owes a relayout owes it because a width, a margin, an inset or an alignment
/// moved — and the size that was computed from the old one is sitting in the box's cache, keyed by
/// the question that was asked rather than by the style it was answered from. Writing the new style
/// and leaving the cache is a box that is laid out at the old width for ever, with the right style
/// in it and no assertion anywhere able to see the difference. So a box whose element owes a
/// relayout has its cached result, and every ancestor's, thrown away as the style is written.
///
/// Only such an element: a hover that moves a colour rewrites styles too, and invalidating the path
/// to the root for it would relayout the document for a change that moves nothing.
pub fn restyle(store: &mut LayoutStore, document: &Document, nodes: &[NodeIndex]) -> u32 {
    let mut moved = 0;
    for &index in nodes {
        let Some(core) = document.store().try_core(index) else {
            continue;
        };
        if core.kind() != NodeKind::Element {
            continue;
        }
        let relayout = core.dirty().own().contains(Dirty::RELAYOUT);
        let source = document.store().key_of(index);
        let node = document.node(index);
        for key in store.boxes_of(source).to_vec() {
            let Some(box_node) = store.get(key) else {
                continue;
            };
            // The boxes reached by descending from another box are refreshed there, with the style
            // they synthesise from. Reaching one from this list too would style it from the wrong
            // owner: a run of generated text is synthesised from its pseudo-element's style and
            // not from the element's, and both name the element as their source.
            let owned = match (box_node.pseudo, box_node.kind) {
                (Some(PseudoKind::Before), _) => node.before_style(),
                (Some(PseudoKind::After), _) => node.after_style(),
                (None, BoxKind::Element) => node.primary_style(),
                (None, BoxKind::Marker) => node.primary_style().as_ref().map(synthesised_style),
                (None, _) => continue,
            };
            let Some(owned) = owned else {
                continue;
            };
            moved += write(store, key, &owned, relayout);
            moved += descend(store, key, &owned, relayout);
        }
    }
    moved
}

/// Writes `style` onto one box, and reports whether that changed anything.
///
/// The test is allocation identity rather than value equality. Two styles that share an allocation
/// are the same cascade result, which is what makes this a pointer comparison on the path a
/// document full of similar elements takes; two that do not may still agree on every property, and
/// rewriting in that case costs one refcount and no downstream work, because every consumer below
/// keys on the property groups rather than on the style as a whole.
fn write(store: &mut LayoutStore, key: BoxKey, style: &ComputedStyle, relayout: bool) -> u32 {
    let Some(node) = store.get_mut(key) else {
        return 0;
    };
    if same(&node.style, style) {
        return 0;
    }
    node.style = style.clone();
    if relayout {
        mark_dirty(store, key);
    }
    1
}

/// Whether two styles are the same cascade result, as opposed to two that merely agree.
fn same(held: &ComputedStyle, style: &ComputedStyle) -> bool {
    core::ptr::eq(core::ptr::from_ref(&**held), core::ptr::from_ref(&**style))
}

/// Refreshes the boxes below `key` that have no style of their own, and reports how many moved.
///
/// `owner` is the style they are synthesised from, which is the style of the box that generated
/// them and not of the box directly above them: a run of text inside an anonymous wrapper inherits
/// from the element the wrapper was created for, because the wrapper itself was never written.
///
/// Both child orders are walked, because they are not the same set. A box that paints nothing of
/// its own is left out of the paint order and appears only in the layout one — the anonymous root
/// of an inline formatting context is exactly that — and a descent over one order alone therefore
/// never reaches it. That box still generates the line fragments the text inside it is drawn by,
/// so it holds the style the painter tints those glyphs with: left at the one it was built with,
/// a label keeps the colour it had when the button was first drawn, whatever the cascade has done
/// to it since.
fn descend(store: &mut LayoutStore, key: BoxKey, owner: &ComputedStyle, relayout: bool) -> u32 {
    let mut moved = 0;
    let mut stack = vec![key];
    let mut seen: rustc_hash::FxHashSet<BoxKey> = rustc_hash::FxHashSet::default();
    let mut synthesised = None;
    while let Some(current) = stack.pop() {
        let Some(node) = store.get(current) else {
            continue;
        };
        let children = children_of(node);
        for child in children {
            if !seen.insert(child) {
                continue;
            }
            let Some(child_node) = store.get(child) else {
                continue;
            };
            // A box that owns a style stops the descent whether or not its cascade moved: it is an
            // element, and an element that was restyled is in the caller's list under its own name.
            if child_node.pseudo.is_some() || child_node.kind == BoxKind::Element {
                continue;
            }
            let style = synthesised.get_or_insert_with(|| synthesised_style(owner));
            moved += write(store, child, style, relayout);
            stack.push(child);
        }
    }
    moved
}

/// One box's children in both orders, each named once.
///
/// The two orders are different sets and not merely different sequences, so the union is what
/// "every box below this one" means here.
fn children_of(node: &BoxNode) -> Vec<BoxKey> {
    let mut children = node.children.clone();
    for key in &node.paint_children {
        if !children.contains(key) {
            children.push(*key);
        }
    }
    children
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;
    use zgui_dom::side::BoxKey;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::{descend, same, write};

    /// A box of the given kind under `parent`, in both child orders.
    fn insert(store: &mut LayoutStore, parent: Option<BoxKey>, kind: BoxKind) -> BoxKey {
        let key = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            kind,
            FormattingContext::Block,
        ));
        if let Some(parent) = parent {
            store.get_mut(key).expect("live").parent = Some(parent);
            let node = store.get_mut(parent).expect("live");
            node.children.push(key);
            node.paint_children.push(key);
        }
        key
    }

    /// A box that lays out under `parent` but paints nothing of its own.
    ///
    /// The anonymous root of an inline formatting context has this shape: it is a layout child of
    /// the element that established the context, while the paint order skips it and names the runs
    /// inside it directly.
    fn insert_unpainted(store: &mut LayoutStore, parent: BoxKey, kind: BoxKind) -> BoxKey {
        let key = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            kind,
            FormattingContext::Inline,
        ));
        store.get_mut(key).expect("live").parent = Some(parent);
        store.get_mut(parent).expect("live").children.push(key);
        key
    }

    #[test]
    fn writing_the_style_a_box_already_holds_reports_no_movement() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let key = insert(&mut store, None, BoxKind::Element);
        let held = store.node(key).style.clone();
        assert_eq!(write(&mut store, key, &held, false), 0);
    }

    #[test]
    fn writing_a_different_allocation_replaces_what_the_box_holds() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let key = insert(&mut store, None, BoxKind::Element);
        let fresh = StyleDraft::initial().build();
        assert_eq!(write(&mut store, key, &fresh, false), 1);
        assert!(same(&store.node(key).style, &fresh));
    }

    /// `element > anonymous wrapper > text run`, with a nested element beside the run.
    #[test]
    fn the_descent_reaches_anonymous_boxes_and_stops_at_the_next_element() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let element = insert(&mut store, None, BoxKind::Element);
        let wrapper = insert(&mut store, Some(element), BoxKind::AnonymousInlineRoot);
        let run = insert(&mut store, Some(wrapper), BoxKind::TextRun);
        let nested = insert(&mut store, Some(wrapper), BoxKind::Element);
        let nested_run = insert(&mut store, Some(nested), BoxKind::TextRun);

        let owner = StyleDraft::initial().build();
        let before = store.node(nested_run).style.clone();
        assert_eq!(descend(&mut store, element, &owner, false), 2);

        let wrapper_style = store.node(wrapper).style.clone();
        assert!(
            same(&wrapper_style, &store.node(run).style),
            "one synthesised style serves the whole run of boxes below one owner"
        );
        assert!(
            same(&before, &store.node(nested_run).style),
            "the descent went through an element and restyled its text from the wrong owner"
        );
    }

    /// `element > anonymous inline root > run`, with the root left out of the paint order.
    ///
    /// This is the shape a paragraph really has, and the one a descent over the paint order alone
    /// cannot reach: the root is the box that generates the line fragments, so the style it holds
    /// is the style every glyph on those lines is tinted with.
    #[test]
    fn a_box_that_paints_nothing_of_its_own_is_still_restyled() {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let element = insert(&mut store, None, BoxKind::Element);
        let root = insert_unpainted(&mut store, element, BoxKind::AnonymousInlineRoot);
        let run = insert(&mut store, Some(root), BoxKind::TextRun);
        // The paint order of the element names the run directly, skipping the root, which is what
        // flattening an inline formatting context produces.
        store
            .get_mut(element)
            .expect("live")
            .paint_children
            .push(run);

        let stale = store.node(root).style.clone();
        let owner = StyleDraft::initial().build();
        assert_eq!(descend(&mut store, element, &owner, false), 2);
        assert!(
            !same(&stale, &store.node(root).style),
            "the box that generates the line fragments kept the style it was built with, so the \
             text on those lines is drawn in a colour the cascade has moved away from"
        );
        assert!(
            same(&store.node(root).style, &store.node(run).style),
            "one synthesised style serves every box below one owner, whichever order it hangs in"
        );
    }
}
