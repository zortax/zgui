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
//!   never settle. Each watched quantity is compared on its own, and an unwatched one is never
//!   measured.
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
            // The handlers' wakes are answered by this flush, as the frame's own flush answers
            // the ones before it.
            self.gate.requests_serviced();
            owed |= zgui_reactive::flush().needs_another_frame;
            self.restyle_and_relayout_after_delivery();
            if pass + 1 == MAX_PASSES {
                // The geometry the last relayout produced has not been delivered. The next frame
                // compares it against what was recorded and delivers it, so one is asked for.
                counter::bump(Counter::ObservationsTruncated);
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
            // Each value on its own, so a view watching one quantity is not handed it again because
            // another one moved.
            if held.mask.contains(ObservedMask::BORDER_BOX)
                && measured.border_box != held.border_box
            {
                self.dom
                    .deliver(node, ObservedValue::BorderBox(measured.border_box));
            }
            if held.mask.contains(ObservedMask::CONTENT_SIZE)
                && measured.content_size != held.content_size
            {
                self.dom
                    .deliver(node, ObservedValue::ContentSize(measured.content_size));
            }
            let scrolled = measured.scroll_offset != held.scroll_offset
                || measured.content_size != held.content_size
                || measured.scrollport != held.scrollport;
            if held
                .mask
                .intersects(ObservedMask::SCROLL_OFFSET | ObservedMask::SCROLLPORT)
                && scrolled
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
    ///
    /// Only the quantities its mask watches are measured. The others keep the values `held` has,
    /// so comparing the result with `held` compares exactly what is watched: a view watching a size
    /// is not woken on every frame its box moves.
    fn measure(&self, key: zgui_dom::NodeKey, held: ObservationSlots) -> Option<ObservationSlots> {
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(key).first()?;
        let resolved = layout.layout_of(first)?;
        let mask = held.mask;
        let scrolls = mask.intersects(ObservedMask::SCROLL_OFFSET | ObservedMask::SCROLLPORT);
        let border_box = if mask.contains(ObservedMask::BORDER_BOX) {
            zgui_layout::fragment::transform::placed::window_box(
                &layout,
                first,
                &self.host.placements(),
            )
            .unwrap_or_else(|| resolved.border_box())
        } else {
            held.border_box
        };
        let (content_size, scroll_offset, scrollport) =
            if scrolls || mask.contains(ObservedMask::CONTENT_SIZE) {
                let region = zgui_layout::scroll_region::region_of(&layout, first);
                let content = region.map_or(resolved.content_box().size, |region| region.content);
                if scrolls {
                    (
                        content,
                        self.scroll.borrow().offset_of(key),
                        region.map_or(resolved.padding_box().size, |region| region.scrollport.size),
                    )
                } else {
                    (content, held.scroll_offset, held.scrollport)
                }
            } else {
                (held.content_size, held.scroll_offset, held.scrollport)
            };
        Some(ObservationSlots {
            mask,
            border_box,
            content_size,
            scroll_offset,
            scrollport,
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
        // Every change made so far is serviced by this frame: the style and layout ones below, and
        // the rest by the paint and accessibility stages that still run after this. Left standing,
        // the flag a handler raised here buys a second frame that damages nothing.
        self.document.borrow().changes_serviced();
        if !self.owes_further_work() {
            return;
        }
        self.restyle();
        // Between the cascade and the layout, exactly as the main pass orders them: a paragraph
        // flattened by the layout below claims its brush slot against the cascade result it was
        // styled by, and a text colour that moved in this cascade has to be written through its
        // slot before that happens. Skipped here, a row born in this settle is shaped into a slot
        // the frame never learns about, and every later colour change misses it.
        self.update_text_brushes();
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
