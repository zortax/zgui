//! The frame's animation stage, and the two things it hands the rest of the frame.
//!
//! It runs before the reactive flush and therefore before the restyle, because both of the things
//! it produces are inputs to what follows: the elements it decided have to cascade again are marked
//! before the traversal looks for work, and the values it wrote for the elements that do not are in
//! place before anything is painted.
//!
//! The lifecycle events it collected are dispatched from here too. They are aimed at the element the
//! animation is running on and travel the ordinary path, so a handler on an ancestor sees them like
//! any other event — which is what makes "unmount when the exit animation actually ends" a listener
//! rather than a guessed duration.
//!
//! The moment the *next* tick is owed at is decided here as well, at the other end of the frame,
//! and it is a phase rather than a delay: see [`cadence`].

pub mod cadence;
mod place;
mod trace;

use zgui_style::AnimationTime;
use zgui_vocab::Timestamp;

use crate::dispatch::{self, HostSink};
use crate::window::Window;

impl Window {
    /// Advances every running animation and applies what that produced.
    ///
    /// Returns how many elements took the repaint-only path, which is what a budget asserting that
    /// a hover transition never restyles is written against.
    ///
    /// `at` is the moment the frame is *for*, taken once when the frame began and handed to every
    /// stage in it. Reading the clock again here would interpolate this frame's values against a
    /// moment a little after the one its phase is anchored to and its events are stamped with —
    /// later by however long the stages before it took, which is not the same on two frames. The
    /// frames themselves stay one refresh apart; what moves is where inside each of them the
    /// animation is sampled, so a motion made of even steps is drawn as uneven ones.
    pub(crate) fn animate(&mut self, at: Timestamp) -> usize {
        let now = AnimationTime(at.since_origin().as_secs_f64());
        let (tick, moved) = {
            let document = self.document.borrow();
            let report = self.engine.animation_tick(&document, now);
            drop(document);
            let mut document = self.document.borrow_mut();
            let layout = self.layout.borrow();
            // The display list is handed to the tick rather than consulted after it, because what
            // an element owes is decided from whether the box could be moved without composing it —
            // and that is answerable only by whatever holds the coordinate systems.
            let mut writing = place::Writing {
                scene: &mut self.scene,
                layout: &layout,
                scale: self.scale,
                damage: zgui_bits::DamageSet::new(),
            };
            let tick = self.animator.tick(&mut document, &report, &mut writing);
            (tick, writing.damage)
        };

        // What the writes moved. Nothing else in the frame will put it in the set: the boxes that
        // moved were not composed again, which is the whole saving, and the pass that would
        // otherwise have absorbed their old and new rectangles never descended to them.
        self.damage.absorb_set(&moved);

        // Asking for a cascade is the engine's to do; deciding which elements need one is not, and
        // the split is what keeps the decision testable without an engine anywhere near it.
        for index in &tick.cascading {
            let document = self.document.borrow();
            self.engine.mark_animation_restyle(&document, *index);
        }

        if !tick.edges.is_empty() {
            // Before the events and not only after the restyle. A handler for `animationend` asks
            // "is anything still running on me?", and the tick above is what made the answer *no*:
            // published only at the end of the frame, the number a handler reads is the one taken
            // before the animation it is being told about had finished. Everything written against
            // that number then waits for an end that has already happened and never comes — which
            // is content kept mounted for its exit animation and never unmounted.
            self.publish_running_animations();
        }
        for edge in tick.edges {
            self.dispatch_edge(&edge, at);
        }
        tick.cheap
    }

    /// Records what this frame leaves owed to whatever is still animating.
    ///
    /// Runs at the end of the frame, because what is animating is not settled until the cascade has
    /// run: an animation this frame's own cascade started is running by the time this is asked, and
    /// one whose last keyframe this frame drew is not.
    ///
    /// `now` is the moment the frame began rather than the moment it ended, because that is the
    /// moment the frame was *for* — every animated value in it was interpolated against it — and a
    /// phase measured from the end of a frame would move with how long the frame took.
    ///
    /// An occluded window keeps nothing. Its animations go on running against the clock and are
    /// drawn correctly whenever it is shown again, but the phase they were on describes a rate the
    /// window was not running at, and an occlusion is not bounded by anything.
    pub(crate) fn pace_animations(&mut self, now: std::time::Instant) {
        if self.occluded || self.starved || !self.is_animating() {
            self.animation.park();
            return;
        }
        self.animation.advance(now, self.refresh_interval());
    }

    /// Marks the animations this frame's cascade created, which its tick could not have seen.
    ///
    /// A keyframe animation is started by the cascade. The tick runs before the cascade, so on the
    /// frame that starts one there is nothing to report and nothing to mark — and a loop with
    /// nothing marked parks for good. This is the only thing that gets the second frame.
    pub(crate) fn note_started_animations(&mut self) {
        let running = self.engine.animations().running_elements();
        if running.is_empty() {
            return;
        }
        let mut document = self.document.borrow_mut();
        self.animator.note_started(&mut document, &running);
    }

    /// Publishes how many animations each element is running, for views to read.
    ///
    /// Without this the answer a view gets is always zero, and everything written against it —
    /// "keep this mounted until its exit animation finishes" above all — takes the no-animation
    /// branch every time and unmounts before a single frame of the exit has been drawn. The count
    /// is taken after the restyle, because the cascade is what creates a keyframe animation.
    pub(crate) fn publish_running_animations(&mut self) {
        let running = self.engine.animations().running_elements();
        let host = std::rc::Rc::clone(&self.host);
        if running.is_empty() {
            // Replaced rather than merged: an element absent from the map is running none, and a
            // map that was only ever added to would report a finished animation for ever.
            host.publish_animations(rustc_hash::FxHashMap::default());
            return;
        }
        let document = self.document.borrow();
        let mut counts =
            rustc_hash::FxHashMap::with_capacity_and_hasher(running.len(), Default::default());
        for index in running {
            let Some(record) = document.store().try_core(index) else {
                continue;
            };
            let count = self.engine.animations().running_on(index);
            if count > 0 {
                counts.insert(record.key(), count);
            }
        }
        drop(document);
        trace::published(&counts);
        host.publish_animations(counts);
    }

    /// Delivers one lifecycle event at the element its animation is running on.
    fn dispatch_edge(&mut self, edge: &zgui_anim::Edge, timestamp: Timestamp) {
        let kind = edge.kind();
        let steps = {
            let document = self.document.borrow();
            if document.store().get(edge.node).is_none() {
                if trace::on() {
                    trace::gone(kind, edge.node);
                }
                return;
            }
            let chain = zgui_input::HitChain::to_root(document.store(), edge.node);
            let mut plan = zgui_input::dispatch::Plan::default();
            zgui_input::dispatch::resolve(document.store(), &chain, kind, &mut plan);
            plan.steps().to_vec()
        };
        trace::edge(kind, edge.node, steps.len());
        if steps.is_empty() {
            return;
        }
        let host = std::rc::Rc::clone(&self.host);
        let mut sink = HostSink::new(&host);
        dispatch::run(
            self,
            &steps,
            kind,
            Some(edge.node),
            &edge.payload,
            &[],
            zgui_vocab::Modifiers::NONE,
            timestamp,
            &mut sink,
        );
    }
}
