//! Handing settled geometry back to the views that asked for it.
//!
//! This is the only path from layout back into the view layer, and a whole class of component
//! cannot be written without one. A popover decides which way to flip from its own measured size
//! and its anchor's box; a virtualised list decides which rows exist from a live scroll offset; a
//! scroll thumb is a function of offset, content extent and scrollport. None of those is
//! answerable from a previous frame's reading, because what the decision changes is exactly the
//! measurement it is made from.
//!
//! Four properties, each load-bearing:
//!
//! * **It runs before anything is painted**, so a repositioned popover is painted in its final
//!   place in the frame it opens. Delivering after paint is the "menu flashes in the wrong corner"
//!   fault, and no later correction removes the frame it was wrong in.
//! * **It is skipped whole when nothing is observing**, which is the state of every node in an
//!   ordinary document — one emptiness test, not a walk.
//! * **A value is delivered only when it changed.** A value delivered again is a signal written
//!   again, which is an effect re-run and a frame; a document with a popover in it would otherwise
//!   never settle.
//! * **It is bounded at two passes.** A popover converges in one: the first lays the positioner
//!   out, the second places it. A cycle is warned about once and truncated.

use zgui_bits::Dirty;
use zgui_dom::side::observed::{ObservationSlots, ObservedMask};
use zgui_profile::{Counter, counter};
use zgui_view::ObservedValue;

use crate::window::Window;

/// The most passes a delivery is allowed to take before it is truncated.
const MAX_PASSES: u8 = 2;

impl Window {
    /// Delivers every observed measurement that changed, and re-runs the reactive work if any did.
    ///
    /// Answers whether the frame owes another: reactive work the delivery's flush could not
    /// finish, or a delivery that did not settle inside the pass budget. Both are otherwise
    /// silent — the flush here runs with wakes folded into the frame, and its outcome is the
    /// only record of what it left behind.
    pub(crate) fn deliver_observations(&mut self) -> bool {
        // The registry is the authority on whether anything is watching. The per-node column is
        // the fast path the walk below probes; it is not a second registry.
        if self.dom.observation_count() == 0 {
            return false;
        }

        let mut owed = false;
        for pass in 0..MAX_PASSES {
            counter::bump(Counter::ObservationPasses);
            if !self.deliver_once() {
                return owed;
            }
            owed |= zgui_reactive::flush().needs_another_frame;
            self.restyle_and_relayout_after_delivery();
            if pass + 1 == MAX_PASSES {
                // The geometry the last relayout produced has not been delivered. The next frame
                // compares it against what was recorded and delivers it, so one is asked for.
                owed = true;
                tracing::warn!(
                    target: "zgui::observe",
                    "geometry observation did not settle in {MAX_PASSES} passes; the frame is \
                     painted against the second one"
                );
            }
        }
        owed
    }

    /// Delivers what changed, and reports whether anything was delivered at all.
    fn deliver_once(&mut self) -> bool {
        let watched: Vec<(zgui_dom::NodeKey, ObservationSlots)> = {
            let document = self.document.borrow();
            let store = document.store();
            self.dom
                .observed_nodes()
                .into_iter()
                .filter_map(|node| {
                    let key = zgui_view_dom::id::to_document(node)?;
                    let slots = store.columns().observed.get(key)?;
                    slots.is_watched().then_some((key, *slots))
                })
                .collect()
        };

        let mut delivered = false;
        for (key, held) in watched {
            let Some(measured) = self.measure(key, held) else {
                continue;
            };
            if measured == held {
                continue;
            }
            delivered = true;
            let node = zgui_view_dom::id::to_view(key);
            if held.mask.contains(ObservedMask::BORDER_BOX) {
                self.dom
                    .deliver(node, ObservedValue::BorderBox(measured.border_box));
            }
            if held.mask.contains(ObservedMask::CONTENT_SIZE) {
                self.dom
                    .deliver(node, ObservedValue::ContentSize(measured.content_size));
            }
            if held
                .mask
                .intersects(ObservedMask::SCROLL_OFFSET | ObservedMask::SCROLLPORT)
            {
                self.dom.deliver(
                    node,
                    ObservedValue::ScrollPosition(zgui_view::ScrollPosition {
                        offset: measured.scroll_offset,
                        content_size: measured.content_size,
                        scrollport: measured.scrollport,
                    }),
                );
            }
            let document = self.document.borrow();
            document
                .edit(&zgui_dom::EverythingMatters, |edit| {
                    let Some(index) = document.store().index_of(key) else {
                        return;
                    };
                    edit.record_observed(index, &measured);
                })
                .expect("the document is not poisoned");
        }
        delivered
    }

    /// What one watched node measures to now.
    fn measure(&self, key: zgui_dom::NodeKey, held: ObservationSlots) -> Option<ObservationSlots> {
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(key).first()?;
        let resolved = layout.layout_of(first)?;
        let region = zgui_layout::scroll_region::region_of(&layout, first);
        Some(ObservationSlots {
            mask: held.mask,
            border_box: zgui_layout::fragment::transform::placed::window_box(
                &layout,
                first,
                &self.host.placements(),
            )
            .unwrap_or_else(|| resolved.border_box()),
            content_size: region.map_or(resolved.content_box().size, |region| region.content),
            scroll_offset: self.scroll.borrow().offset_of(key),
            scrollport: region.map_or(resolved.padding_box().size, |region| region.scrollport.size),
        })
    }

    /// Restyles and re-composes after a delivery wrote something, if it actually changed anything.
    ///
    /// A delivery is not a change. Most of what a frame delivers is read by a view that computes
    /// the same answer it computed last time — a thumb whose fraction of the track is unchanged, a
    /// height republished as the same number of pixels — and the signal write that carries it marks
    /// nothing at all. Running the tail of the pipeline for that costs a second and a third
    /// complete layout of the document per frame, against the one an idle frame runs, and produces
    /// a display list identical to the one already there.
    ///
    /// So the tail runs only when something in the document actually owes work. The test is the
    /// root's own invalidation word, which is the union of every obligation below it: it is the
    /// widest possible reading and it is still a single load. When it says nothing is owed, nothing
    /// is owed anywhere, and the frame's own layout — which ran before this, with this frame's
    /// scroll offsets in it — is the answer.
    pub(crate) fn restyle_and_relayout_after_delivery(&mut self) {
        if !self.owes_further_work() {
            return;
        }
        self.restyle();
        self.build_boxes();
        self.lay_out();
    }

    /// Whether anything in the document owes work that a further pass would service.
    ///
    /// Every phase the tail of the frame runs, and no other: styling, box construction, layout and
    /// the two text phases. A repaint or an accessibility projection is not among them — those are
    /// serviced later in the *same* frame, by stages that run after this one.
    fn owes_further_work(&self) -> bool {
        const SERVICED: Dirty = Dirty::RESTYLE
            .union(Dirty::RECASCADE)
            .union(Dirty::REBUILD_BOX)
            .union(Dirty::CHILDREN)
            .union(Dirty::RELAYOUT)
            .union(Dirty::RESHAPE)
            .union(Dirty::REBREAK);
        let document = self.document.borrow();
        let Some(root) = document.root_index() else {
            return false;
        };
        let dirty = document.store().core(root).dirty();
        (dirty.own() | dirty.subtree()).intersects(SERVICED)
    }
}
