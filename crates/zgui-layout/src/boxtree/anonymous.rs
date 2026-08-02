//! The boxes CSS requires that no element names.
//!
//! A block container whose children are inline-level does not lay them out itself: each maximal run
//! of them becomes one anonymous box, and that box establishes the inline formatting context the
//! run is broken into lines in. Without the wrapper the run's boxes would be handed straight to an
//! algorithm that stacks every child on a line of its own, which is what a paragraph of text must
//! never do.

use zgui_css::{ComputedStyle, inherited_style};
use zgui_dom::side::BoxKey;

use crate::node::box_node::BoxNode;
use crate::node::kind::{BoxKind, FormattingContext};
use crate::style::convert::display::Participation;
use crate::tree::store::LayoutStore;

/// A style whose inherited properties come from `parent` and whose others are at their initial
/// values.
///
/// This is what an anonymous box is styled with, and it is not the parent's style: an anonymous
/// wrapper must not inherit its parent's borders, background or padding, which would then be
/// painted twice.
///
/// What it inherits it *shares* rather than copies, which is what lets the run of text inside it be
/// recognised as the element's own: the brush slot its glyphs are drawn through is claimed against
/// the identity of the cascade result the colour came from, so a wrapper holding an equal copy of
/// that result would take every string in the document out of reach of the colour written for it.
pub fn synthesised_style(parent: &ComputedStyle) -> ComputedStyle {
    inherited_style(parent)
}

/// One child of a container, as the wrapper pass sees it.
#[derive(Clone, Copy, Debug)]
pub struct Placed {
    /// The box.
    pub key: BoxKey,
    /// How it takes part in its container's formatting context.
    pub participation: Participation,
    /// Whether it is laid out by an ancestor rather than by the box it was written inside.
    ///
    /// An out-of-flow box still belongs to its writer's paint order, so it is carried here rather
    /// than dropped: the wrapping pass has to keep it out of the layout list and leave it in the
    /// paint one.
    pub out_of_flow: bool,
}

/// Wraps each maximal run of inline-level children in one anonymous box.
///
/// A container whose children are all block-level is returned unchanged and allocates nothing,
/// which is the common case in a component library.
pub fn wrap_inline_runs(
    store: &mut LayoutStore,
    parent_style: &ComputedStyle,
    children: &[Placed],
) -> Vec<BoxKey> {
    if !children
        .iter()
        .any(|child| child.participation == Participation::Inline && !child.out_of_flow)
    {
        return children.iter().map(|child| child.key).collect();
    }
    let mut wrapped = Vec::with_capacity(children.len());
    let mut run: Vec<BoxKey> = Vec::new();
    for child in children {
        if child.out_of_flow {
            flush(store, parent_style, &mut run, &mut wrapped);
            wrapped.push(child.key);
            continue;
        }
        match child.participation {
            Participation::Inline => run.push(child.key),
            _ => {
                flush(store, parent_style, &mut run, &mut wrapped);
                wrapped.push(child.key);
            }
        }
    }
    flush(store, parent_style, &mut run, &mut wrapped);
    wrapped
}

/// Turns the run collected so far into one anonymous box, if there is one.
fn flush(
    store: &mut LayoutStore,
    parent_style: &ComputedStyle,
    run: &mut Vec<BoxKey>,
    out: &mut Vec<BoxKey>,
) {
    if run.is_empty() {
        return;
    }
    let taken = core::mem::take(run);
    let mut wrapper = BoxNode::new(
        synthesised_style(parent_style),
        BoxKind::AnonymousInlineRoot,
        FormattingContext::Inline,
    );
    wrapper.children = taken.clone();
    wrapper.paint_children = taken.clone();
    wrapper.block_level = true;
    let key = store.insert(wrapper);
    for child in taken {
        if let Some(node) = store.get_mut(child) {
            node.parent = Some(key);
            node.parent_fc = FormattingContext::Inline;
        }
    }
    out.push(key);
}
