//! Who is watching which of a node's measurements.
//!
//! The document keeps a mask per node saying *whether* anything is watching, because that question
//! is asked of every node whose box moved and has to be free. It cannot keep the answer to *who*:
//! a delivery channel is a reference-counted closure, and the store that mask lives in is shared
//! with worker threads that may hold neither.
//!
//! So this is the other half. It holds the channels, counts how many observers each measurement
//! has, and tells the document whenever a count crosses zero — which is what keeps the mask an
//! honest answer rather than a stale one.

use std::rc::Rc;

use rustc_hash::FxHashMap;
use zgui_dom::Document;
use zgui_dom::side::observed::ObservedMask;
use zgui_view::{NodeId, ObservationSink, Observed, ObservedValue};

/// One registration's number, which is what deregisters it.
pub type Registration = u64;

/// Everything watching one measurement of one node.
#[derive(Default)]
struct Watchers {
    /// The channels, by the number that removes each.
    sinks: Vec<(Registration, ObservationSink)>,
}

/// The observers this backend is holding.
#[derive(Default)]
pub struct Observations {
    /// One entry per watched measurement of one node.
    entries: FxHashMap<(NodeId, Observed), Watchers>,
    /// The next registration number, never reused.
    next: Registration,
}

impl Observations {
    /// A registry with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing at all is being watched.
    ///
    /// What a test asserts after everything that was observing has been dropped: an entry left
    /// behind keeps a view's signal alive and goes on being asked about a node that is gone.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many measurements are being watched, across every node.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every node something is watching, each named once.
    ///
    /// This is the authority on *which* nodes a frame has to measure. The document's own column
    /// answers the same question per node and is the fast path a geometry walk probes; this is the
    /// list, and a frame that walked the document instead would pay for every node in it to
    /// service the two that are being watched.
    pub fn nodes(&self) -> Vec<NodeId> {
        let mut nodes: Vec<NodeId> = self.entries.keys().map(|(node, _)| *node).collect();
        nodes.sort_unstable();
        nodes.dedup();
        nodes
    }

    /// Records `sink` as watching `what` of `node`.
    ///
    /// Returns the registration's number and the node's whole mask afterwards. The mask goes to
    /// the document; the number comes back to deregister with.
    pub fn add(
        &mut self,
        node: NodeId,
        what: Observed,
        sink: ObservationSink,
    ) -> (Registration, ObservedMask) {
        self.next += 1;
        let registration = self.next;
        self.entries
            .entry((node, what))
            .or_default()
            .sinks
            .push((registration, sink));
        (registration, self.mask_of(node))
    }

    /// Removes one registration, and reports the node's whole mask afterwards.
    ///
    /// `None` when there was no such registration, which is what a second drop of the same handle
    /// would be — and which must not be turned into a mask write, because the mask it would write
    /// is the one that is already there.
    pub fn remove(
        &mut self,
        node: NodeId,
        what: Observed,
        registration: Registration,
    ) -> Option<ObservedMask> {
        let watchers = self.entries.get_mut(&(node, what))?;
        let position = watchers
            .sinks
            .iter()
            .position(|(held, _)| *held == registration)?;
        watchers.sinks.remove(position);
        if watchers.sinks.is_empty() {
            self.entries.remove(&(node, what));
        }
        Some(self.mask_of(node))
    }

    /// Hands `value` to everything watching that measurement of `node`.
    ///
    /// Called by whatever produced the measurement, once it is settled and before anything is
    /// painted, so a view that moves itself in response is painted in its final place in the same
    /// frame rather than one frame late.
    pub fn deliver(&self, node: NodeId, value: ObservedValue) {
        let Some(watchers) = self.entries.get(&(node, value.observed())) else {
            return;
        };
        // Cloned before calling, because a sink writes a signal and a signal write can reach code
        // that registers or drops an observation of its own.
        let sinks: Vec<ObservationSink> = watchers
            .sinks
            .iter()
            .map(|(_, sink)| Rc::clone(sink))
            .collect();
        for sink in sinks {
            sink(value);
        }
    }

    /// Forgets every observation of a node `document` no longer has.
    ///
    /// The same reasoning as for a listener's handler: a view that took its nodes out of the
    /// document has nothing left to deregister against, and an entry kept past its node goes on
    /// holding a signal that nothing will ever write again.
    pub fn retain_live(&mut self, document: &Document) {
        self.entries
            .retain(|(node, _), _| crate::id::is_live(document, *node));
    }

    /// Which of `node`'s measurements are being watched.
    fn mask_of(&self, node: NodeId) -> ObservedMask {
        let mut mask = ObservedMask::empty();
        for what in [
            Observed::BorderBox,
            Observed::ContentSize,
            Observed::ScrollPosition,
        ] {
            if self.entries.contains_key(&(node, what)) {
                mask |= bit(what);
            }
        }
        mask
    }
}

/// Which bit of the document's mask one measurement is.
fn bit(what: Observed) -> ObservedMask {
    match what {
        Observed::BorderBox => ObservedMask::BORDER_BOX,
        Observed::ContentSize => ObservedMask::CONTENT_SIZE,
        // A scroll position is an offset and a scrollport together, so watching it watches both.
        Observed::ScrollPosition => ObservedMask::SCROLL_OFFSET | ObservedMask::SCROLLPORT,
        // A measurement this build does not know how to record is watched here and nowhere else:
        // the mask stays as it was, so the document never claims to be watching something it will
        // not deliver.
        _ => ObservedMask::empty(),
    }
}
