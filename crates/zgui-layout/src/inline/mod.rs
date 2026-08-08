//! Lines: turning a run of text, images and nested spans into boxes with positions.
//!
//! # What is a leaf here and what is not
//!
//! To the algorithms around it an inline formatting context is one leaf box with a size. Inside it
//! is a tree — text runs, images, `inline-block`s, spans nested inside spans, each with its own
//! font, margins and alignment — and none of those is laid out on its own. The box that
//! *establishes* the context lays all of them out together, in one pass over one shaped paragraph,
//! because a line break is a decision about the whole sequence and cannot be taken piecewise.
//!
//! A box whose parent already establishes a context is therefore never asked for its own size. It
//! is reached through its context and nowhere else.
//!
//! | Module | What it decides |
//! |---|---|
//! | [`strut`] | how tall each run makes the line it is on |
//! | [`lines`] | the CSS line boxes, and where they stack |
//! | [`ellipsis`] | where a line that does not fit is cut off, and what marks the cut |
//! | [`vertical_align`] | how far each box moves off the baseline |
//! | [`floats`] | how wide each line is allowed to be |
//! | [`baseline`] | which line a parent aligns the whole context by |
//! | [`resolved`] | what the context came out at, for everything that reads layout afterwards |
//! | [`atomic`] | the nested layout an `inline-block` costs, and the memo without which it is ruinous |
//!
//! Flattening the tree into the string a shaper takes, sizing what is on the line that is not a
//! glyph, and emitting one fragment per line are internal to the context: none of the three is a
//! question anything outside it asks.

pub mod atomic;
pub mod baseline;
pub(crate) mod boxes;
pub(crate) mod content;
pub mod ellipsis;
pub mod floats;
pub(crate) mod insets;
pub mod leaf_shell;
pub mod lines;
pub(crate) mod measure;
pub mod resolved;
pub mod strut;
pub mod vertical_align;

use std::sync::Arc;

use taffy::{BlockContext, NodeId, RequestedAxis, RunMode, Size};
use zgui_dom::side::BoxKey;

use crate::inline::content::Generated;
use crate::inline::content::memo::Flattened;
use crate::key::from_node_id;
use crate::measure::{MeasureContent, MeasureRequest, Measured};
use crate::node::kind::FormattingContext;
use crate::tree::LayoutTree;

/// Measures one leaf box's content.
///
/// Two very different things arrive here. An inline formatting context is laid out in full, because
/// its size *is* the lines it broke into. Replaced content — an image, a video, an embedded surface
/// — is asked of whoever is driving the pass, because this engine does not own it.
pub(crate) fn measure_leaf<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    node: NodeId,
    known: Size<Option<f32>>,
    available: Size<taffy::AvailableSpace>,
    run_mode: RunMode,
    axis: RequestedAxis,
    block: Option<&mut BlockContext<'_>>,
) -> Measured {
    let key = from_node_id(node);
    if tree.store().node(key).fc == FormattingContext::Custom {
        // A custom element's measurement is the trait's, not the measurer's: the source was
        // installed on the pass, and the shell arithmetic around this call applies to its answer
        // exactly as it applies to a replaced box's.
        return crate::custom::measure(
            tree,
            key,
            known,
            available,
            run_mode == RunMode::PerformLayout,
        );
    }
    if tree.store().node(key).fc == FormattingContext::Inline {
        return measure::compute(
            tree,
            key,
            measure::Ask {
                known,
                available,
                run_mode,
                axis,
                final_pass: run_mode == RunMode::PerformLayout,
            },
            block,
        );
    }
    let scale = tree.device().scale;
    let style = tree.store().node(key).style.clone();
    let natural = tree
        .store()
        .replaced(key)
        .and_then(|content| content.natural);
    tree.content().measure(MeasureRequest {
        box_: key,
        style: &style,
        known,
        available,
        natural,
        scale,
        final_pass: run_mode == RunMode::PerformLayout,
    })
}

/// The flattened form of one inline formatting context, built if the box is not already holding one
/// that was built from the same content.
///
/// Flattening walks every character in the context, and what comes out depends on the boxes, their
/// styles and the device scale — never on the width the context is being asked about. The
/// algorithms around a paragraph ask it how big it is at a dozen widths, and lay the document out
/// again whenever anything anywhere moves, so the flattened form is kept beside the box and reused
/// until the content it was built from is no longer what the box holds.
pub(crate) fn content_of<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
) -> Arc<Generated> {
    let scale = tree.device().scale;
    let pieces = content::collect::pieces(tree.store(), key);
    if let Some(held) = tree
        .store()
        .flattened(key)
        .and_then(|held| held.reuse(scale, &pieces))
    {
        return held;
    }
    let built = {
        let (store, measurer, styles) = tree.content_parts();
        let mut claim = |paint: &zgui_text_style::TextPaint| measurer.paint_slot(paint);
        Arc::new(content::generate::build(
            store, key, &pieces, styles, &mut claim, scale,
        ))
    };
    tree.store_mut()
        .hold_flattened(key, Flattened::new(scale, pieces, Arc::clone(&built)));
    built
}
