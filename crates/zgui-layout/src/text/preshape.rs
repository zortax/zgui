//! Finding the paragraphs a layout pass is about to shape, before it runs.
//!
//! Shaping happens inside the layout recursion, which is serial. A frame that dirtied many
//! paragraphs — the first frame dirties all of them — can shape them on workers instead, if it
//! knows which they are before layout asks. This walk answers that: it descends exactly the
//! dirty region, flattens each dirty inline context on the frame thread, and hands back the
//! shaping questions layout would otherwise ask one at a time.
//!
//! Only paragraphs with no inline items are collected. An atomic inline's width, and an inline
//! box edge's resolved margins, are measurements the pass has not taken yet, and both are part
//! of the shaping key. A paragraph of plain runs is a pure function of its flattened form and
//! the scale, so its key here is its key during layout, and the cache fills exactly as serial
//! shaping would have filled it.
//!
//! Flattening stays on the frame thread deliberately: it claims brush slots, and a slot is an
//! identity later frames compare against, so claiming must stay deterministic. The flattened
//! form is memoised on the box, so the pass that follows reuses it rather than flattening again.

use std::sync::Arc;

use zgui_dom::side::BoxKey;
use zgui_text::{ParagraphContent, ParagraphKey};

use crate::inline::content::Generated;
use crate::measure::MeasureContent;
use crate::node::kind::FormattingContext;
use crate::tree::LayoutTree;
use crate::tree::store::LayoutStore;

/// The paragraphs a pass is about to shape, with everything their shaping questions borrow.
#[derive(Debug, Default)]
pub struct PreShapeJobs {
    /// Device pixels per CSS pixel, which is part of every question.
    scale: f32,
    /// The questions, keyed, owning their flattened forms.
    jobs: Vec<(ParagraphKey, Arc<Generated>)>,
}

impl PreShapeJobs {
    /// Whether there is nothing to shape.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// How many paragraphs were collected.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// The questions, in the form a shaper takes.
    pub fn contents(&self) -> Vec<(ParagraphKey, ParagraphContent<'_>)> {
        self.jobs
            .iter()
            .map(|(key, generated)| {
                (
                    *key,
                    ParagraphContent {
                        text: &generated.text,
                        map: &generated.map,
                        runs: &generated.runs,
                        boxes: &[],
                        paragraph: &generated.paragraph,
                        scale: self.scale,
                    },
                )
            })
            .collect()
    }
}

/// Collects the dirty plain-text paragraphs of the document, flattening each.
pub fn collect<C: MeasureContent>(tree: &mut LayoutTree<'_, C>) -> PreShapeJobs {
    let scale = tree.device().scale;
    let dirty = dirty_inline_boxes(tree.store());
    let mut jobs = Vec::with_capacity(dirty.len());
    for key in dirty {
        let generated = crate::inline::content_of(tree, key);
        // An item is an atomic inline or an inline-box edge; either one's geometry is part of
        // the shaping key and is not measured yet. Those paragraphs shape during layout.
        if !generated.items.is_empty() {
            continue;
        }
        let content = ParagraphContent {
            text: &generated.text,
            map: &generated.map,
            runs: &generated.runs,
            boxes: &[],
            paragraph: &generated.paragraph,
            scale,
        };
        let paragraph_key = generated.key(&content);
        jobs.push((paragraph_key, Arc::clone(&generated)));
    }
    PreShapeJobs { scale, jobs }
}

/// Every dirty box that establishes an inline formatting context.
///
/// Dirtiness is upward-closed, so a clean box has a clean subtree and the walk skips it whole;
/// the cost is the dirty region and the clean boxes on its edge, never the document.
fn dirty_inline_boxes(store: &LayoutStore) -> Vec<BoxKey> {
    let Some(root) = store.root() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if !crate::tree::dirty::is_dirty(store, key) {
            continue;
        }
        let node = store.node(key);
        // Establishing an inline context, as opposed to being inline content in one: a nested
        // span is `Inline` too, and flattening it alone would shape a paragraph the pass never
        // asks for.
        if node.fc == FormattingContext::Inline && node.parent_fc != FormattingContext::Inline {
            out.push(key);
        }
        stack.extend(node.children.iter().copied());
    }
    out
}
