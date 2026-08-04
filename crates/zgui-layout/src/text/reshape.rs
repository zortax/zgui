//! Invalidating the text of a few elements, rather than of the document.
//!
//! A shaped paragraph carries a brush slot in its glyphs, chosen when it was shaped. An element
//! given a *different* slot — because the one it was sharing cannot be rewritten on its behalf —
//! therefore has shaping that names the wrong brush, and the only way to correct it is to shape
//! that text again. Everything measured from that shaping goes with it: the cached sizes of the
//! boxes the text is in, their baselines, and the lines the inline formatting context around it
//! resolved.
//!
//! What this module exists to bound is *which* boxes that is. The argument above is about one
//! element, and answering it by dropping every shaped paragraph in the window and marking every box
//! in the layout tree turns one control changing colour into a whole-document reflow. So the reach
//! is stated here instead, once, in the two directions it actually has:
//!
//! * **down**, through the boxes below the element that hold its text — a run of characters, the
//!   anonymous wrappers around it, the inline formatting context it establishes;
//! * **up**, because the paragraph the text belongs to may be established by an ancestor, and every
//!   box between the two was sized from the answer that paragraph gave.
//!
//! The walk is generous in both directions rather than exact, for the reason the tree itself gives:
//! which box establishes the context a run of characters belongs to is not a question with one
//! answer, and each step is a handful of pointer writes against a chain as long as the tree is
//! deep. It is not generous *across*: a sibling that shares nothing with the element pays nothing,
//! and that is the whole of the difference.

use rustc_hash::FxHashSet;
use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};
use zgui_text::ParagraphKey;

use crate::node::box_node::BoxNode;
use crate::tree::store::LayoutStore;

/// What re-brushing a set of elements costs the layout tree.
#[derive(Clone, Debug, Default)]
pub struct Reshape {
    /// The shaped paragraphs whose glyphs named a brush that has been replaced.
    ///
    /// Every one of them has to be dropped by whoever holds the shaping, and this is the list that
    /// says which — the layout tree knows which contexts were invalidated and does not own a single
    /// glyph.
    pub paragraphs: Vec<ParagraphKey>,
    /// How many boxes had a held answer thrown away.
    pub boxes: u32,
}

/// Invalidates the text of every element in `nodes`, and reports what that reached.
///
/// Boxes are counted once however many elements reach them, which is what keeps a subtree whose
/// elements all re-brushed together — an inherited colour moving is exactly that — linear in the
/// subtree rather than quadratic in it.
pub fn scope(store: &mut LayoutStore, nodes: impl IntoIterator<Item = NodeKey>) -> Reshape {
    let mut reshape = Reshape::default();
    let mut seen: FxHashSet<BoxKey> = FxHashSet::default();
    let roots: Vec<BoxKey> = nodes
        .into_iter()
        .flat_map(|node| store.boxes_of(node).to_vec())
        .collect();

    // Downwards first, so that an element inside another element's subtree finds the shared part
    // already invalidated and stops there instead of walking it again.
    let mut stack = roots.clone();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(node) = store.get(key) else {
            continue;
        };
        stack.extend(children_of(node));
        invalidate(store, key, &mut reshape);
    }

    // And upwards from each element's own boxes. An ancestor that has already been reached was
    // reached either as a descendant of another element in this set — whose own ancestors are
    // walked by this same loop — or by an earlier walk that carried on to the root, so there is
    // nothing above it left to do.
    for root in roots {
        let mut next = store.get(root).and_then(|node| node.parent);
        while let Some(key) = next {
            if !store.contains(key) || !seen.insert(key) {
                break;
            }
            next = store.node(key).parent;
            invalidate(store, key, &mut reshape);
        }
    }

    counter::add(Counter::BoxesReshaped, u64::from(reshape.boxes));
    reshape
}

/// Throws away everything one box is holding that was computed from shaped text.
///
/// Four things go together and none of them may be left behind. The **flattened form** of the
/// context is the string the shaper was handed, and it holds the brush each run was to be drawn
/// with — kept, it would hand the replaced brush straight back to the reshaping it caused. The
/// **resolution** is the lines that came out, which name the shaped paragraph by a key that is
/// about to stop resolving. The **baselines** were read off those lines. And the **cache** is the
/// sizes the box answered from all three, which is what a later pass would serve instead of asking
/// again.
fn invalidate(store: &mut LayoutStore, key: BoxKey, reshape: &mut Reshape) {
    if !store.contains(key) {
        return;
    }
    store.forget_flattened(key);
    if let Some(resolution) = store.take_inline_resolution(key) {
        reshape.paragraphs.push(resolution.key);
    }
    let state = store.state_mut(key);
    state.first_baseline = None;
    state.last_baseline = None;
    state.forget_layout();
    reshape.boxes += 1;
}

/// One box's children in both orders, each named once.
///
/// The two orders are different sets rather than different sequences: a box that paints nothing of
/// its own appears in one and not the other, and the anonymous root of an inline formatting context
/// is exactly that — so a descent over either alone would walk past the box holding the paragraph.
fn children_of(node: &BoxNode) -> Vec<BoxKey> {
    let mut children = node.children.clone();
    for key in &node.paint_children {
        if !children.contains(key) {
            children.push(*key);
        }
    }
    children
}
