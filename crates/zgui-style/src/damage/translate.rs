//! Turning the style engine's per-element damage into this framework's obligations.

use rustc_hash::FxHashMap;
use style::selector_parser::RestyleDamage;
use zgui_bits::Dirty;
use zgui_css::ComputedStyle;
use zgui_dom::dirty::propagate;
use zgui_dom::{DocumentStore, NodeIndex};

use crate::damage::bits;
use crate::damage::layout_damage::{TextKeyStore, TextWork};
use crate::damage::{a11y_key, paint_key};
use crate::driver::traversal::Restyled;

/// Where the obligations one restyle produced are collected before they are written.
///
/// Collected rather than written as they are decided, because writing an obligation walks to the
/// root and the walk is what makes a mark visible to the stage that services it — doing that from
/// inside the loop that reads the engine's data would interleave two different kinds of access to
/// the document.
///
/// One entry per node, folded together as they arrive. The fold is by lookup rather than by
/// scanning what is already there: an element is given several obligations from several sources,
/// a document contributes as many entries as it has elements, and a scan per obligation is
/// quadratic in the size of the document — which is paid in full on the pass that styles a fresh
/// one, where the collection costs many times the style engine it is recording.
#[derive(Default)]
pub struct DamageSink {
    /// What each node owes.
    marks: Vec<(NodeIndex, Dirty)>,
    /// Where each node's own entry is.
    at: FxHashMap<NodeIndex, usize>,
    /// What the subtree below each node owes.
    subtrees: Vec<(NodeIndex, Dirty)>,
    /// Where each node's subtree entry is.
    below: FxHashMap<NodeIndex, usize>,
}

/// Folds `bits` into `node`'s entry of `entries`, adding one if it has none.
fn fold(
    entries: &mut Vec<(NodeIndex, Dirty)>,
    at: &mut FxHashMap<NodeIndex, usize>,
    node: NodeIndex,
    bits: Dirty,
) {
    match at.get(&node) {
        Some(&index) => entries[index].1 |= bits,
        None => {
            at.insert(node, entries.len());
            entries.push((node, bits));
        }
    }
}

impl DamageSink {
    /// An empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `node` owes `bits`.
    pub fn mark(&mut self, node: NodeIndex, bits: Dirty) {
        fold(&mut self.marks, &mut self.at, node, bits);
    }

    /// Records that everything below `node` owes `bits`.
    pub fn mark_subtree(&mut self, node: NodeIndex, bits: Dirty) {
        fold(&mut self.subtrees, &mut self.below, node, bits);
    }

    /// How many nodes were given an obligation.
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// Whether nothing was damaged.
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty() && self.subtrees.is_empty()
    }

    /// Writes every recorded obligation into the document, and empties the sink.
    pub fn apply(&mut self, store: &mut DocumentStore) {
        // The indices go with the entries they point at: a sink emptied of its entries but not of
        // its index would fold the next restyle's first obligation into a slot that no longer
        // exists.
        self.at.clear();
        self.below.clear();
        for (node, bits) in self.marks.drain(..) {
            propagate::mark(store, node, bits);
        }
        for (node, bits) in self.subtrees.drain(..) {
            // The subtree half only: the node's own obligations are whatever its own arm decided,
            // and widening them here would rebuild a box nothing asked about.
            store.core(node).dirty().mark_subtree(bits);
            propagate::propagate(store, node, bits);
        }
    }
}

/// Turns one restyled element's damage into obligations.
///
/// Three sources, and none of them is redundant with the others.
///
/// **A first-time cascade produces no engine damage at all.** The engine returns before
/// accumulating any when there is no old style to compare against, and per-element damage starts
/// empty. Without the first branch below a newly mounted subtree gets no relayout and no box, and
/// never appears. It covers more than an insertion, too: a subtree coming back out of `display:
/// none` has had its style data thrown away, so every element in it is styled for the first time
/// again with no mutation at any of them.
///
/// **The engine's four bits are a nested lattice**, not four independent flags — relayout contains
/// recalculate-overflow, which contains rebuild-stacking-context, which contains repaint. So the
/// arms are tested widest first and each level implies the ones below it. A flat sequence of tests
/// would fire every arm for every relayout; leaving the middle arms out gives an *empty* answer
/// for `transform`, `rotate`, `scale`, `translate`, `perspective`, `isolation` and `z-index`,
/// because the embedder hook never fires for them and nothing else would mark anything.
///
/// **A relayout is not a rebuild.** The embedder's own bits separate the two: a width, a margin or
/// an inset carries [`RELAYOUT_BOX`](crate::damage::bits::RELAYOUT_BOX) alone and throws away a
/// cached measurement, while a `display`, a `position` or a generated-content string carries the
/// construction bits and throws away boxes. Only the second is allowed to reach `mark_subtree`,
/// because that mark propagates to the root and the stage that services it rebuilds everything the
/// mark leads to.
///
/// **The engine's relayout bit is not this pipeline's relayout.** It is set for a border colour, a
/// corner radius, a box shadow and a mask, because the layout the engine was written for keeps
/// painting fragments inside its boxes and rebuilds them. This one does not, and taking the bit at
/// its word rebuilt every box in the document for a colour. So the widest arms are entered on the
/// embedder's own bits, which the engine fills in by calling back into the document with both
/// styles in hand — and under a relayout the engine's narrower bits are excluded, because the
/// lattice means it is carrying every one of them regardless of what the change was.
///
/// **The engine's repaint bit is not the paint predicate, and must not become one.** Border
/// colours, corner radii, visibility, masks and box shadows carry no damage annotation at all, so
/// a hover that changes a border colour arrives with empty damage. The key comparison is what
/// covers those, and it is valid for every restyle rather than only for layout-affecting ones.
pub fn translate(
    store: &mut DocumentStore,
    texts: &mut TextKeyStore,
    record: &Restyled,
    style: &ComputedStyle,
    out: &mut DamageSink,
) {
    let node = record.index;

    // Every mark below is written once per restyled element, which is a different order of
    // magnitude from the frame trace they share a ring with — so they are behind their own switch
    // rather than behind whether anything is listening at all. A restyle of a large document
    // writes more of them in one frame than a ring holds, which loses the frame boundaries the
    // ring was kept for; and a panel that draws the trace is drawing elements, which is a loop.
    let why = zgui_profile::latency::tracing_elements();

    if record.initial {
        if why {
            zgui_profile::latency::mark("why.initial");
        }
        out.mark(node, Dirty::RELAYOUT | Dirty::REBUILD_BOX);
    }

    // The engine's relayout bit is the widest thing it can say, not the narrowest: it is set for a
    // border colour and for a corner radius, because the layout it was written for keeps painting
    // fragments inside its boxes. What that change costs *this* pipeline is in the embedder's own
    // bits, which the engine fills in by calling back into the document — so the relayout arm is
    // entered on the engine's bit and decided on ours.
    if record.damage.intersects(bits::ALL) {
        out.mark(node, Dirty::RELAYOUT);
        // The embedder's own bits exist only under a relayout, which is the only condition the
        // engine calls for them under.
        if why {
            zgui_profile::latency::note(
                "why.relayout",
                format!("{:?}", record.damage).replace('"', "'"),
            );
        }
        if record.damage.contains(bits::CONSTRUCT_BOX) {
            if why {
                zgui_profile::latency::mark("why.construct_box");
            }
            out.mark(node, Dirty::REBUILD_BOX);
        }
        if record.damage.contains(bits::CONSTRUCT_DESCENDANTS) {
            if why {
                zgui_profile::latency::mark("why.construct_descendants");
            }
            // The node's own half as well as the subtree's. The stage that services a rebuild
            // visits the elements that *own* the obligation and makes their boxes again, subtree
            // included — so a mark that raised only the subtree union would be retired by that
            // stage's walk without a single box being made, and the descendants it named would
            // keep the boxes the change was about.
            out.mark(node, Dirty::REBUILD_BOX);
            out.mark_subtree(node, Dirty::REBUILD_BOX);
        }
        // The engine's hook is handed two styles and no memory, so it can only say "this may cost
        // a shape". Here there is a memory: an element whose shaping key did not move cannot need
        // a fresh shape, and the difference between the two answers is about twenty-eight times
        // the work.
        let text = texts.record(record.node, style);
        // The widest damage the engine can report is every bit of the word at once, and it means
        // something the element's own keys cannot see: a generated-content box that has started or
        // stopped existing. The text laid out under this element changed without any property of
        // this element's style moving, so this is the one input the narrowing above may not be
        // applied to — narrowed, generated content appears unshaped, or one mutation late.
        let widest = record.damage.contains(RestyleDamage::reconstruct());
        if record.damage.contains(bits::RESHAPE_TEXT) && (widest || text == TextWork::Reshape) {
            out.mark(node, Dirty::RESHAPE);
        }
        if record.damage.contains(bits::REBREAK_TEXT) && (widest || text != TextWork::None) {
            out.mark(node, Dirty::REBREAK);
        }
    } else if record.damage.contains(bits::RECALCULATE_INK)
        || (!record.damage.contains(RestyleDamage::RELAYOUT)
            && record.damage.contains(RestyleDamage::RECALCULATE_OVERFLOW))
    {
        // transform, rotate, scale, translate, transform-origin, perspective-origin and the text
        // decorations, which the engine classifies itself; and the corner radii, box shadows,
        // clips and masks it calls a relayout, which this pipeline reads only while a fragment is
        // measured. Nothing moves, but the ink and the scrollable overflow do.
        //
        // The engine's arms are a nested lattice, so a relayout contains this bit whatever it was
        // about. Under a relayout the embedder's own bits are the whole answer and the engine's
        // are excluded, or a change classified here as paint-only would arrive as an overflow
        // recalculation instead of as nothing.
        out.mark(node, Dirty::REFRAGMENT | Dirty::REHIT | Dirty::REPAINT);
    } else if !record.damage.contains(RestyleDamage::RELAYOUT)
        && record
            .damage
            .contains(RestyleDamage::REBUILD_STACKING_CONTEXT)
    {
        // z-index, and anything else that changes whether the element establishes a stacking
        // context.
        out.mark(node, Dirty::RESTACK | Dirty::REHIT | Dirty::REPAINT);
    }

    let paint = paint_key::paint_key(style, record.pseudos);
    let key = store.key_of(node);
    let a11y = a11y_key::a11y_key(store, node, style);
    let previous = store
        .columns_mut()
        .paint_key
        .insert(key, paint)
        .unwrap_or(zgui_dom::side::paint_key::PaintStyleKey::UNSTYLED);
    if previous != paint {
        out.mark(node, Dirty::REPAINT);
        // A generated-content style is cloned into the box that carries it, so a change to one
        // rebuilds that box rather than merely repainting the element it hangs off.
        if paint_key::pseudos_moved(previous, paint) {
            if why {
                zgui_profile::latency::mark("why.pseudos_moved");
            }
            out.mark(node, Dirty::REBUILD_BOX);
        }
    }

    if store
        .columns_mut()
        .a11y_key
        .insert(key, a11y)
        .unwrap_or(zgui_dom::side::a11y_key::A11yKey::UNPROJECTED)
        != a11y
    {
        out.mark(node, Dirty::A11Y);
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;

    use super::DamageSink;

    #[test]
    fn one_node_gets_one_entry_however_many_sources_marked_it() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let other = document.append(root, NodeKind::Element, ElementName::new("box"));

        let mut sink = DamageSink::new();
        sink.mark(root, Dirty::RELAYOUT);
        sink.mark(other, Dirty::REPAINT);
        // Out of order on purpose: obligations for one element arrive from several places, and a
        // sink that folded only into the entry it pushed last would grow one entry per obligation.
        sink.mark(root, Dirty::REPAINT);
        assert_eq!(sink.len(), 2);

        sink.apply(document.store_mut());
        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::RELAYOUT | Dirty::REPAINT),
            "both obligations reached the node"
        );

        // And the index is emptied with the entries: a stale one would fold the next restyle's
        // first obligation into a slot that is no longer there.
        assert!(sink.is_empty());
        sink.mark(other, Dirty::A11Y);
        assert_eq!(sink.len(), 1);
    }
}
