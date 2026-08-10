//! One frame, from the events that arrived to the park that follows.
//!
//! The order is the whole content of this module, and every step of it is load-bearing:
//!
//! | Step | What it does |
//! |---|---|
//! | drain | the events that arrived since the last frame, dispatched into the document |
//! | timers | every callback whose deadline has passed, in deadline order |
//! | device | the surface reconfigured, then the device the cascade is matched against |
//! | animate | every running animation moved on, and what that owes marked |
//! | flush | the reactive work, which is where effects write the document |
//! | restyle | the elements that owe one, and nothing else |
//! | brushes | the text colours the cascade moved, written through their slots |
//! | boxes | the box tree, patched where the content changed and rebuilt where the structure did |
//! | layout | measure and arrange, then compose fragments and diff them |
//! | deliver | geometry to whoever asked to be told about it |
//! | rehit | what is under a pointer that has not moved but now has something else under it |
//! | publish | the brushes, into the display list the emit walk reads them from |
//! | paint | the damage grown over what reads outside itself, then emitted against it |
//! | draw | the display list, scissored to the damage, presented |
//! | announce | the accessibility tree, and only when something is listening to it |
//! | park | one request for another frame if anything owes one, and the deadline to wait on |
//!
//! Two of them exist entirely because of a bug that has no other symptom. **Delivering geometry
//! before painting** is what stops a menu appearing for one frame in the wrong corner. **Re-hit
//! testing under a stationary pointer** is what stops a dropdown that opens under the cursor from
//! never being hovered.

use std::time::Instant;

use zgui_bits::{DamageSet, Dirty};
use zgui_geom::{Device, DevicePx, Size};
use zgui_layout::style::DeviceStyle;
use zgui_layout::tree::LayoutTree;
use zgui_paint::PaintInput;
use zgui_platform::Clock;
use zgui_profile::{Counter, counter};
use zgui_render::{FrameOutcome, RenderTarget, SkipReason};

use crate::window::Window;

/// What one frame did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameReport {
    /// What the renderer made of it.
    pub outcome: FrameOutcome,
    /// Whether anything asked for another frame from inside this one.
    pub needs_another_frame: bool,
    /// How many elements the restyle touched.
    pub restyled: usize,
    /// How many timer callbacks fired.
    pub timers_fired: usize,
    /// How many elements were animated without being styled again.
    pub animated: usize,
}

/// What a parked window deadline services.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeadlineKind {
    /// Timers, tasks, animation, resize or presentation produce a normal frame.
    Render,
    /// Cold-resource trimming only; no paint or presentation is requested.
    Maintenance,
}

/// One window's earliest fixed deadline and what reaching it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScheduledDeadline {
    pub(crate) at: Instant,
    pub(crate) kind: DeadlineKind,
}

impl Window {
    /// Queues a platform event for the next frame to dispatch, and reports whether it needs one.
    ///
    /// Events are not dispatched where they arrive. Dispatching means running handlers, and a
    /// handler's writes have to settle in a frame — so the frame is asked for, and the frame is
    /// where the events are drained.
    ///
    /// An event that describes a state the window is already in needs no frame. A window system
    /// repeats itself: a drag delivers the same extent more than once, and a compositor re-states
    /// an occlusion that has not changed. Every one of those used to run the whole pipeline and
    /// present a surface identical to the one already on the screen.
    ///
    /// A **size** is not an event at all but a level, and it is treated as one. A configure that
    /// moves it always records the new size; whether it also asks for a frame is
    /// [`ResizePace`](crate::ResizePace)'s decision, and the answer is no while the last resize
    /// frame is younger than one frame of the output. The obligation is not dropped — it is
    /// carried by `reconfigure` and discharged by [`Window::merged_deadline`], and the frame that
    /// runs then is built for whatever the window is at that moment rather than for the configure
    /// that installed the deadline.
    pub fn queue(&mut self, event: zgui_platform::SurfaceEvent) -> bool {
        let wants_a_frame = match &event {
            zgui_platform::SurfaceEvent::Resized(size) => self.resized(*size, self.scale),
            zgui_platform::SurfaceEvent::ScaleFactorChanged { scale_factor, size } => {
                self.resized(*size, *scale_factor as f32)
            }
            zgui_platform::SurfaceEvent::Occluded(occluded) => {
                let moved = self.occluded != *occluded;
                // Un-occluding forces a full redraw: nothing observed what the compositor did to
                // the surface while it was hidden.
                if self.occluded && !*occluded {
                    self.damage = DamageSet::full();
                    self.reconfigure = true;
                }
                self.occluded = *occluded;
                self.handle.set_occluded(*occluded);
                moved
            }
            // A **colour scheme** is a level too, and the one that decides which rules match: it is
            // recorded here so that the frame this asks for is built against a device that answers
            // `prefers-color-scheme` the way the desktop now does. Recording it nowhere is a live
            // theme flip that changes no pixel in the window, ever, because the media query that
            // selects the dark rules keeps answering with the scheme the window launched in.
            zgui_platform::SurfaceEvent::ColorSchemeChanged(scheme) => {
                self.set_platform_color_scheme(*scheme)
            }
            // Keyboard focus is a level as well, and it is one whose *edges* do work that must not
            // be done twice: losing it settles the field being typed into, which is the event a
            // form validates on, and a window system that re-states the focus it already reported
            // would settle it again.
            zgui_platform::SurfaceEvent::Focused(focused) => {
                let moved = self.surface_focused != *focused;
                self.surface_focused = *focused;
                self.handle.set_focused(*focused);
                moved
            }
            _ => true,
        };
        // Focus is not input from a person, but what it does — ending a composition, announcing a
        // settled value — runs handlers, and handlers run in a frame rather than where an event
        // arrives. So it is queued with the input and drained with it.
        if event.is_input() || (wants_a_frame && Self::surface_focus_of(&event).is_some()) {
            self.queued.push(event);
        }
        wants_a_frame
    }

    /// Runs one frame.
    ///
    /// The whole of it runs with the window's own scope current, which is what makes the free
    /// functions a component is written with — scheduling a callback, reading which node holds
    /// focus, observing a box — work from a listener's body, from an effect and from another
    /// callback rather than only while the view is being built. They resolve the window they
    /// belong to through the scope they are called in, and a frame that ran outside it would give
    /// every one of them nothing to resolve against: in a debug build a panic naming the wrong
    /// thing, and in a release build a tooltip that silently never opens.
    pub fn frame(&mut self, clock: &dyn Clock) -> FrameReport {
        match self.scope.as_ref().map(|scope| scope.owner().clone()) {
            Some(owner) => owner.with(|| self.run_frame(clock)),
            None => self.run_frame(clock),
        }
    }

    /// The frame itself, with the window's scope already current.
    fn run_frame(&mut self, clock: &dyn Clock) -> FrameReport {
        use zgui_profile::latency::mark;
        mark("f.begin");
        let now = clock.now();
        let timestamp = clock.timestamp();

        self.frame_started();
        self.gate.begin_frame();
        self.document.borrow().begin_frame();
        // Both are this frame's answers about this frame's movement, and a frame that inherited
        // either would decide what to draw from what the last one moved.
        self.rigid_moves = zgui_layout::fragment::diff::RigidMoves::default();
        self.damage_before_layout = DamageSet::new();
        self.layout_passes = 0;
        self.scrolled_this_frame.clear();

        // The events that arrived since the last frame, and the commands their handlers issued.
        zgui_profile::latency::note_with("f.drain", || self.queued.len().to_string());
        // What the drain settled between its own events can leave work behind, exactly as the
        // flush below it can, and a frame that forgot it would park with that work undone.
        let drained = self.drain_input(timestamp);
        // After the drain, so that a press that started a drag is the press this ends.
        self.end_press_after_drag();
        // Timers before the reactive work, so what a callback writes settles in this same frame.
        mark("f.timers");
        let timers_fired = self.fire_timers(now);
        // Everything moving on its own: a smooth scroll on its way, an edge springing back. It
        // marks the containers it moved and nothing else, exactly as a wheel event does.
        mark("f.scroll");
        self.advance_scroll(timestamp);
        // A finger held still becomes a long press by the clock and by nothing else, so the frame
        // asks — and leaves behind the one moment it next owes an answer at, which the park merges
        // in. A held contact is not an animation and must not be parked on as one.
        mark("f.gestures");
        self.advance_gestures(now, timestamp);
        // The surface first and the device second: what the cascade matches against is derived
        // from the surface this frame will actually present to.
        mark("f.reconfigure");
        self.reconfigure_surface();
        mark("f.device");
        self.device_epoch();
        // Before the flush and therefore before the restyle: the elements this decides have to
        // cascade again are marked before the traversal goes looking for work, and the values it
        // writes for the ones that do not are in place before anything is painted.
        mark("f.animate");
        let animated = self.animate(timestamp);

        // Which changes can affect a computed style is answered from an index the restyle
        // repopulates, so the frame in which the sheets changed takes the full path throughout.
        self.engine.disable_filters_if_sheets_changed();

        // Everything asked for up to this line is answered by the flush that follows it: a
        // handler that ran in this frame's own dispatch, a timer's callback, a task that finished
        // while the frame was starting. What arrives after it owes a frame of its own.
        self.gate.requests_serviced();
        mark("f.flush");
        let flush = zgui_reactive::flush();
        mark("f.commands");
        self.binding.checkpoint();
        self.carry_out_commands(timestamp);
        // After the flush, which is where a finished decode's task pushed its result, and before
        // the line below that closes the frame's window for producing changes: a picture that
        // landed during this frame's own flush is shown by this frame, one settle later. A settle
        // that *kicked* decodes is owed one more frame — the tasks it spawned have never been
        // polled, and an unpolled future has no waker for its completion to fire.
        mark("f.images");
        if self.images.settle(&self.document, &mut self.content) {
            self.request_frame();
        }
        // The encoded bytes live `ImageBytes` handles hold, published where the loader has just
        // dealt with everything else an image costs: a memory report reads all three figures —
        // encoded, decoded, tiles — from one frame.
        counter::set(Counter::EncodedImageBytes, zgui_image::registry_bytes());

        // Everything above this line is a stage that *produces* changes — events dispatched,
        // timers fired, the reactive graph flushed, commands carried out — and everything below it
        // is a stage that consumes them. Saying so here is what stops an interaction costing two
        // frames: the mutation a handler made during this frame's own dispatch is shown by this
        // frame, and the flag it raised would otherwise still be standing at the end of it and buy
        // another frame that damages nothing.
        self.document.borrow().changes_serviced();

        mark("f.restyle");
        let restyled = self.restyle();
        zgui_profile::latency::note_with("f.restyled", || restyled.to_string());
        // The cascade is what starts a keyframe animation, and the tick above ran before it. An
        // element whose animation this frame created is therefore in no report and owes nothing;
        // this is what asks the loop to come back for it.
        self.note_started_animations();
        // What is running is published where the cascade has just decided it, so a view asking
        // "is anything still animating on this node" is answered about this frame.
        self.publish_running_animations();
        // After the cascade, because `cursor` is a computed value and a `:hover` rule is what most
        // documents write it in — asking before the restyle would answer with the style the
        // element had before the pointer arrived on it, which is one frame stale for the whole
        // time the pointer is there.
        self.update_cursor();
        mark("f.brushes");
        self.update_text_brushes();
        zgui_profile::latency::note_with("d.boxes", || {
            format!(
                "full={} n={} area={:?} rects={:?}",
                self.damage.is_full(),
                self.damage.len(),
                self.damage.area(),
                self.damage.rects()
            )
        });
        mark("f.boxes");
        self.build_boxes();
        zgui_profile::latency::note_with("d.prelayout", || {
            format!(
                "full={} n={} area={:?} rects={:?}",
                self.damage.is_full(),
                self.damage.len(),
                self.damage.area(),
                self.damage.rects()
            )
        });
        mark("f.layout");
        self.lay_out();
        zgui_profile::latency::note_with("d.postlayout", || {
            format!(
                "full={} n={} area={:?} rects={:?}",
                self.damage.is_full(),
                self.damage.len(),
                self.damage.area(),
                self.damage.rects()
            )
        });
        // After layout, because a picture's demand *is* its laid-out content box: this is the
        // first moment the loader can know how many device pixels each image actually needs, and
        // the decode it kicks is sized by the answer.
        mark("f.image_demand");
        if self
            .images
            .observe_demand(&self.layout.borrow(), self.scale)
        {
            self.request_frame();
        }
        // After layout, because a surface that has just opened has only now got the boxes that
        // make the controls inside it reachable at all.
        mark("f.enter");
        self.enter_owed_focus_traps(timestamp);
        mark("f.dispatch_scroll");
        self.dispatch_scroll(timestamp);
        mark("f.observe");
        self.deliver_observations();
        // A second settle, for what the scroll and observation deliveries produced: both flush
        // and relayout mid-frame precisely so a virtualised list renders this frame's rows, and
        // rows born that way write their `src` after the first settle has run. Without this, each
        // such row's first paint shows no picture — one blank frame per remount, which a fast
        // glide turns into visible flicker. The demand pass runs again for the same reason: the
        // decode for a fresh source is kicked this frame rather than next. The marks the settle
        // files are consumed next frame; the pictures are drawn from this one.
        mark("f.images_late");
        if self.images.has_arrivals() && self.images.settle(&self.document, &mut self.content) {
            self.request_frame();
        }
        if self
            .images
            .observe_demand(&self.layout.borrow(), self.scale)
        {
            self.request_frame();
        }
        mark("f.rehit");
        self.rehit();
        mark("f.publish_brushes");
        self.publish_text_brushes();
        // After layout, because where a caret goes is a question about the lines this frame
        // produced; before the paint, because what it damages has to be in the set the emit walk
        // is gated on. A caret that blinked moves no fragment at all, so nothing else in the frame
        // would ever put its rectangle into the damage — and a rectangle nobody damaged is a caret
        // that is drawn once and then never changes again.
        mark("f.caret");
        self.plan_caret(now);
        // After layout and the caret plan, before paint emission: the embed host binds producers
        // to laid-out boxes, attaches whatever they presented, and absorbs the damage the emit
        // walk below is gated on. See the `embed` module for the whole argument.
        mark("f.embed");
        self.sync_embeds(timestamp);

        self.binding.before_paint(timestamp);
        mark("f.paint");
        let outcome = self.paint_and_draw();
        mark("f.painted");
        // Immediately after the frame that produced them, and against the output this window is
        // actually on: what the acquisition made this frame wait for is how much earlier than
        // necessary it was started, and whether it finished inside one frame of the output is
        // whether there was anything to schedule into at all.
        let cost = clock.now().saturating_duration_since(now);
        let interval = self.refresh_interval();
        self.present
            .observed(self.renderer.acquire_block(), cost < interval, interval);
        // Named for what it counts. A window with nothing moving in it is supposed to leave this
        // alone, so the assertion written against it — that a still document draws nothing over
        // three hundred refreshes — reads as zero when it holds rather than as three hundred.
        if outcome.retires_damage() {
            counter::bump(Counter::FramesDrawn);
        }

        // A frame that never reached the renderer keeps its damage; every other outcome retires it,
        // because what retires damage is the work having been submitted rather than a frame having
        // been presented.
        if outcome.retires_damage() {
            self.damage = DamageSet::new();
        }

        // After the renderer, and before the arenas are recycled. After, because what a screen
        // reader is told about where a control is has to be where it was drawn rather than where
        // it was about to be; before, because a removal's parent is re-projected out of records
        // that `end_frame` is what drops.
        mark("f.a11y");
        self.publish_a11y();

        mark("f.recycle");
        self.dom.end_frame();
        // The box arena ends its frame here for the same reason the document's does, and it has to
        // be here rather than inside `lay_out`: a removed box stays readable until this call, and
        // every stage between layout and now — scroll dispatch, geometry observations, re-hit,
        // the caret, paint, the accessibility walk — may still resolve a box key it captured
        // earlier in the frame.
        //
        // A frame where `lay_out` returned before the fragment diff leaves the removed boxes
        // undrained, so their coordinate systems are given back a frame later than usual. That is
        // safe rather than merely tolerable: the list of retired keys is independent of the arena,
        // so the next diff drains the same values, and a `PropertyOwner` carries the slot's
        // occupancy counter — a name given back late can never be confused with one since reissued.
        self.layout.borrow_mut().recycle();
        let changed_during = self.document.borrow().end_frame();
        // After every stage that can write the document and before the frame's own park decision:
        // what this records is what another window's sweep compares against to find out whether
        // this window still owes a frame for something written from over there.
        self.serviced_document();
        let owed = self.gate.end_frame();
        let needs_another_frame = owed || changed_during || flush.needs_another_frame || drained;

        // The renderer's own answer is a source too, and it is the only one that speaks for the
        // frame *after* the work was submitted. An acquisition that timed out, one against a surface
        // the compositor has replaced underneath the frame, one the graphics API rejected and a
        // device that had to be rebuilt all leave the window showing the picture it had before —
        // while the damage that would have corrected it is retired, because the work *was* submitted
        // and the composed target does hold this frame's pixels. Nothing else in the list above
        // knows a frame is owed: the document did not change, no timer is due, and the reactive
        // graph is settled. So the window stops redrawing until something unrelated happens to ask,
        // which is a freeze that ends when the pointer crosses the window, or never.
        //
        // It is owed *later* rather than now, and that is the whole difference between this source
        // and every other one. What these outcomes report is the presentation engine having nothing
        // to hand over, and that is resolved by the compositor coming back to the surface rather
        // than by this process asking again sooner. Asked again straight away, the frame runs the
        // whole pipeline and then waits inside the acquisition for as long as the graphics API is
        // willing to wait — on the thread that reads input — and the next one does it again: one
        // stall becomes a run of them, and the window answers nothing throughout.
        //
        // [`FrameOutcome::wants_another_frame`] is what decides, and it is false for exactly the
        // outcomes that must not ask at all: an occluded surface, a frame that damaged nothing, and
        // a device that could not be rebuilt.
        self.retry_after = outcome
            .wants_another_frame()
            .then(|| now + self.refresh_interval());

        // Measured from when the last normal frame finished, so a continuously active document
        // never trims a working set and a parked one gets exactly one maintenance wake.
        self.maintenance_due = Some(clock.now() + std::time::Duration::from_secs(2));

        // After the cascade and therefore after everything that can start or finish an animation,
        // and against the moment the frame was built for rather than the moment it ended. This is
        // the only thing that gets an animation its next frame: an animating window asks for no
        // frame of its own — that would be a spin at whatever rate the machine can manage — so what
        // brings the loop back is the deadline this leaves, and a window that stopped animating
        // leaves none at all.
        self.pace_animations(now);

        // The one place an in-frame request becomes a real one. An occluded surface is excluded,
        // and that exclusion is the anti-spin rule: honouring it there is a hidden window running
        // the whole pipeline at full rate for ever.
        if needs_another_frame && outcome != FrameOutcome::Skipped(SkipReason::Occluded) {
            self.request_frame();
        }

        if self.first_frame && matches!(outcome, FrameOutcome::Presented(_)) {
            // A surface is shown by its first frame, never before it: showing an unpainted one is
            // what produces a flash of empty window at launch.
            self.surface.set_visible(true);
            self.first_frame = false;
        } else if self.first_frame && matches!(outcome, FrameOutcome::Skipped(SkipReason::Occluded))
        {
            // The converse deadlock, real on macOS: a hidden window's layer hands out no
            // drawable, so the present that would show the window can never happen behind it.
            // The composed target already holds the frame's pixels, so the window is shown now
            // and recorded as occluded. The platform's visibility report then forces the redraw
            // that presents; the direct request covers a surface that was ready immediately.
            self.surface.set_visible(true);
            self.first_frame = false;
            self.occluded = true;
            self.request_frame();
        }

        // Last, with everything this frame produced still standing: the scene it emitted, the
        // damage it answered, the layout it computed. The handle is cloned out first because the
        // probe is handed the window it is stored on.
        if let Some(probe) = self.probe.clone() {
            probe.frame_ended(self);
        }

        zgui_profile::latency::note_with("f.end", || {
            format!(
                "{outcome:?} another={needs_another_frame} owed={owed} \
                 changed={changed_during} reactive={}",
                flush.needs_another_frame
            )
            .replace('"', "'")
        });
        FrameReport {
            outcome,
            needs_another_frame,
            restyled,
            timers_fired,
            animated,
        }
    }

    /// When this window next wants to be woken, if it wants to be woken at all.
    ///
    /// The merge of four sources and no more: the next animation tick, the earliest scheduled
    /// callback, the moment a contact being held becomes a long press, and the moment a configure
    /// that was too soon to answer becomes worth answering. An **occluded** surface keeps its timer
    /// deadline and loses its animation deadline — the frame still has to run so a callback fires
    /// and a sleeping task resumes, but a hidden window animating at full rate is precisely what an
    /// occluded surface must not do.
    ///
    /// The third is a *moment* rather than a tick, and that is what separates it from the first:
    /// something animating owes a frame every refresh interval, and a finger resting on the screen
    /// owes exactly one, when it has been there long enough to mean something else.
    ///
    /// The fourth exists only while a reconfiguration is owed. A window whose size is settled owes
    /// nothing here, and must: a deadline installed on a window with nothing to do is a loop that
    /// wakes for ever and draws nothing.
    ///
    /// **Every one of them is a moment, and not one of them is derived from `now`.** This is asked
    /// on every turn of the loop and not only after a frame, so a source that answered "an interval
    /// from now" would be moved forward by every wake the window had for any other reason — and a
    /// window seeing pointer samples or a compositor re-stating something has several per interval.
    /// The animation's moment therefore comes from the phase the last frame left behind, the
    /// resize's from the frame that last answered a configure, and the blink's from the caret's own
    /// origin. `now` is passed only so that a source can answer whether it has anything left to owe.
    pub fn merged_deadline(&self, now: Instant) -> Option<Instant> {
        self.scheduled_deadline(now).map(|deadline| deadline.at)
    }

    /// The earliest deadline, keeping maintenance distinct from render-producing work.
    pub(crate) fn scheduled_deadline(&self, now: Instant) -> Option<ScheduledDeadline> {
        let interval = self.refresh_interval();
        let animation = self.animation.due().filter(|_| !self.occluded);
        let timer = self.timers.borrow().peek_for(self.dom.document_id(), now);
        let resize = if self.reconfigure {
            self.pace.due(interval)
        } else {
            None
        };
        // The blink is excluded while the surface is hidden, for the same reason the animation
        // deadline is: a caret blinking behind a minimised window is a loop running at two frames a
        // second, for ever, drawing nothing anyone can see.
        let blink = (!self.occluded)
            .then(|| self.carets.next_flip(now))
            .flatten();
        // A frame being held back owes a moment of its own, and it is the only source that owes one
        // for a frame that has already been asked for. Without it a window whose whole reason to
        // draw is the input it has already queued is held and never woken again.
        let held = self.present.due();
        // The frame the renderer asked for and could not be given, owed one refresh interval after
        // the one that failed. Excluded while the surface is hidden for the same reason the
        // animation and the blink are: a window nobody can see must not keep running the pipeline
        // to find out whether the compositor would take a frame yet.
        let retry = self.retry_after.filter(|_| !self.occluded);
        let render = [
            animation,
            timer,
            self.gesture_deadline(),
            resize,
            blink,
            held,
            retry,
        ]
        .into_iter()
        .flatten()
        .min();
        // No source is woken for before the moment the window would stop refusing the frame it
        // asks for. While a reconfiguration is owed and could not yet be seen,
        // [`Window::wants_a_frame`] refuses *every* frame whatever asked for it — so a moment
        // earlier than that is not an early frame but no frame at all: the loop wakes, asks, is
        // refused, computes the same moment again and wakes again, for the rest of the interval,
        // with nothing drawn.
        //
        // This can only bite a source whose moment is fixed rather than derived from the present
        // one, which is every source there is: an animation on a phase laid down before the resize
        // began, a timer whose callback came due while the pointer was still dragging the edge.
        //
        // A held frame is a gate of exactly the same kind and is treated as one: while one is being
        // held every frame is refused, so a moment before it is released is a wake that draws
        // nothing.
        let render = render.map(|earliest| {
            [resize, held]
                .into_iter()
                .flatten()
                .fold(earliest, Instant::max)
        });
        match (render, self.maintenance_due) {
            (Some(render), Some(maintenance)) if render <= maintenance => Some(ScheduledDeadline {
                at: render,
                kind: DeadlineKind::Render,
            }),
            (Some(_), Some(maintenance)) => Some(ScheduledDeadline {
                at: maintenance,
                kind: DeadlineKind::Maintenance,
            }),
            (Some(render), None) => Some(ScheduledDeadline {
                at: render,
                kind: DeadlineKind::Render,
            }),
            (None, Some(maintenance)) => Some(ScheduledDeadline {
                at: maintenance,
                kind: DeadlineKind::Maintenance,
            }),
            (None, None) => None,
        }
    }

    /// Services a maintenance-only deadline without building or presenting a frame.
    pub(crate) fn maintain(&mut self, timestamp: zgui_vocab::Timestamp) {
        self.maintenance_due = None;
        self.renderer.release_idle_resources();
        let mut cx = crate::embed::EmbedMaintenanceCx {
            renderer: &mut *self.renderer,
            content: &mut self.content,
            timestamp,
        };
        self.embed.maintain(&mut cx);
    }

    /// Runs the embed host's sync step, and folds what it reported into the animation gate.
    fn sync_embeds(&mut self, timestamp: zgui_vocab::Timestamp) {
        let mut cx = crate::embed::EmbedSyncCx {
            document: &self.document,
            layout: &self.layout,
            renderer: &mut *self.renderer,
            content: &mut self.content,
            damage: &mut self.damage,
            intrinsics: &self.replaced_surfaces,
            revision: self.dom.revision(),
            scale: self.scale,
            viewport: self.extent.map_or_else(
                || zgui_geom::Size::new(0, 0),
                |extent| {
                    zgui_geom::Size::new(
                        extent.width.0.round().max(0.0) as i32,
                        extent.height.0.round().max(0.0) as i32,
                    )
                },
            ),
            occluded: self.occluded,
            timestamp,
            waker: &self.waker,
        };
        let report = self.embed.sync(&mut cx);
        self.embed_animating = report.animating;
    }

    /// Whether anything in the document is animating.
    ///
    /// One bit, read once. It is the sole input to the animation deadline, which is why it is
    /// marked by both the cheap and the expensive animation paths rather than only by the one that
    /// restyles.
    pub fn is_animating(&self) -> bool {
        if self.is_scrolling() {
            return true;
        }
        // An embedded producer that asked to run every refresh is an animation by any measure the
        // cadence cares about: it wants the next vsync, and it stops wanting it by not asking.
        if self.embed_animating {
            return true;
        }
        let document = self.document.borrow();
        let Some(root) = document.root_index() else {
            return false;
        };
        let dirty = document.store().core(root).dirty();
        (dirty.own() | dirty.subtree()).contains(Dirty::ANIMATING)
    }

    /// Ends the press the router is tracking when the desktop took over a drag.
    ///
    /// A move or resize the compositor drives swallows the pointer: no release ever arrives, so a
    /// button left `:active` stays `:active` and a capture is never given back. Wayland delivers a
    /// focus loss that already does this, and X11 and Windows keep focus throughout — which is why
    /// the drag records that it began rather than relying on an event.
    fn end_press_after_drag(&mut self) {
        if self.handle.take_drag_started() {
            self.cancel_press();
        }
    }

    /// Fires every callback whose deadline has passed, in deadline order.
    fn fire_timers(&mut self, now: Instant) -> usize {
        let due = self.timers.borrow_mut().due(self.dom.document_id(), now);
        if due.is_empty() {
            return 0;
        }
        counter::add(Counter::TimersFired, due.len() as u64);
        for callback in &due {
            // Inside the non-reactive zone, exactly as a listener's body is: a callback that reads
            // a signal is reading it, not subscribing whatever scope happens to be current.
            let _zone = zgui_reactive::enter_non_reactive_zone();
            callback();
        }
        due.len()
    }

    /// Reconfigures the renderer for the surface, when the surface moved.
    ///
    /// The extent is read from the **surface** and never from the event that asked for the frame.
    /// That is what makes a superseded configure cost nothing rather than cost a frame that is
    /// thrown away afterwards: however many arrived while this frame was being asked for, the one
    /// this builds for is the one the window is now.
    fn reconfigure_surface(&mut self) {
        if !self.reconfigure {
            return;
        }
        self.reconfigure = false;
        self.configured += 1;
        // Before the swapchain rebuild rather than after it, so the interval that paces the next
        // configure is a frame period of the output and not a frame period plus this frame's cost.
        self.pace.answered(self.clock.now());
        let size = self.surface.size();
        zgui_profile::latency::note_with("w.cfg", || {
            format!(
                "surface={}x{} scale={} viewport_css={}x{} mhz={:?} interval_us={}",
                size.width.0,
                size.height.0,
                self.scale,
                self.viewport.width.0,
                self.viewport.height.0,
                self.surface.refresh_rate_millihertz(),
                self.refresh_interval().as_micros()
            )
        });
        self.renderer.configure(RenderTarget::new(
            Size::new(size.width.0 as i32, size.height.0 as i32),
            zgui_geom::Scale::new(self.scale),
        ));
        self.damage = DamageSet::full();
    }

    /// Rebuilds the device the cascade is matched against, when the surface moved.
    fn device_epoch(&mut self) {
        let mut document = self.document.borrow_mut();
        let epoch = self.engine.device_epoch(&mut document, self.viewport);
        if epoch.changed {
            tracing::debug!(
                target: "zgui::style",
                origins = ?epoch.origins,
                relaid_out = epoch.relaid_out,
                "the device was rebuilt"
            );
        }
    }

    /// Takes a configure: records the new level, and reports whether it asks for a frame now.
    ///
    /// Two separate refusals live here, and they refuse for different reasons.
    ///
    /// A configure that **does not move the level** is dropped outright. A drag delivers the same
    /// size more than once — a quarter of them, measured over a drag across a monitor — and every
    /// repeat used to rebuild the swapchain, which stalls until the device is idle, and then
    /// repaint the whole surface.
    ///
    /// A configure that **moves the level too soon** is recorded and not answered yet. Its frame
    /// would be built, painted and presented inside the interval in which the last one is still on
    /// its way to the screen, so nothing could ever look at it; the next configure would supersede
    /// it before the scan-out that might have shown it. What it leaves behind — the level, and the
    /// obligation in `reconfigure` — is what the deadline discharges, and the frame that discharges
    /// it reads the surface rather than this event.
    fn resized(&mut self, size: Size<DevicePx, Device>, scale: f32) -> bool {
        if !self.resize(size, scale) {
            return false;
        }
        // What the handle reports, from the size that was actually taken. Maximising and leaving
        // full screen both resize the window, so this is where those move too — no desktop reports
        // them as events of their own.
        self.handle.set_geometry(size, scale);
        self.handle.refresh_window_state();
        let admitted = self.pace.admit(self.clock.now(), self.refresh_interval());
        if !admitted {
            // Nothing is latched here. What refuses the frames the backend produces on its own
            // account — winit turns every configure into a redraw request — is the obligation in
            // `reconfigure` measured against the pace, which is a level like the size itself, and
            // which therefore also refuses the frames that anything *else* asks for while a
            // reconfiguration is owed and could not yet be seen.
            zgui_profile::latency::note_with("w.resize.deferred", || {
                format!(
                    "{}x{} n={}",
                    size.width.0,
                    size.height.0,
                    self.pace.deferred()
                )
            });
        }
        admitted
    }

    /// Records a new surface extent, and reports whether it moved.
    fn resize(&mut self, size: Size<DevicePx, Device>, scale: f32) -> bool {
        if self.extent == Some(size) && scale == self.scale {
            zgui_profile::latency::mark("w.resize.same");
            return false;
        }
        self.extent = Some(size);
        zgui_profile::latency::note_with("w.resize", || {
            format!(
                "device={}x{} scale={} was_scale={} live_surface={}x{} live_scale={}",
                size.width.0,
                size.height.0,
                scale,
                self.scale,
                self.surface.size().width.0,
                self.surface.size().height.0,
                self.surface.scale_factor()
            )
        });
        self.rescale(scale, size.width.0, size.height.0);
        self.reconfigure = true;
        true
    }

    /// Styles the elements that owe it, and reports how many were touched.
    ///
    /// The cascade's result is put onto the boxes those elements already generated before this
    /// returns, and that is not bookkeeping. A box holds a *clone* of the style it was built with,
    /// every stage after layout reads the box's copy, and a cascade produces a fresh allocation
    /// every time it runs — so an element restyled while its box is kept leaves that box painting
    /// from the previous cascade. The frame then damages exactly the right rectangle and redraws it
    /// in the colour that was already there, which no damage assertion anywhere can see.
    ///
    /// The one case that is skipped is a document with no box tree at all, which is the first frame
    /// of every window: every box is about to be built from this very cascade, so writing the
    /// styles onto boxes that do not exist yet is a pass over the styled elements that buys
    /// nothing. A document that *has* a tree is always written to, even when part of it is about to
    /// be rebuilt — the rebuild replaces the subtrees that owe one and no others, and an element
    /// restyled outside them would otherwise keep painting from the previous cascade.
    pub(crate) fn restyle(&mut self) -> usize {
        let mut document = self.document.borrow_mut();
        let pass = self.engine.restyle(&mut document, None);
        let mut layout = self.layout.borrow_mut();
        if pass.styled > 0 && layout.root().is_some() {
            zgui_layout::boxtree::patch::restyle(&mut layout, &document, &pass.styled_nodes());
        }
        pass.styled
    }

    /// Writes the text colours the cascade moved through the slots that name them.
    ///
    /// This is the one step of the restyle's tail the style engine cannot own: the table belongs
    /// to the display list, and the style engine does not depend on it. It runs here — after the
    /// cascade has settled and before anything is laid out — because a paragraph cached from an
    /// earlier frame has to resolve to the new colour in the frame the cascade changed it, and
    /// re-shaping to achieve that is exactly what the indirection exists to avoid.
    fn update_text_brushes(&mut self) {
        let updates = self.engine.text_paint_updates();
        if updates.is_empty() {
            return;
        }
        self.brushes_moved = true;
        let split = super::brushes::apply(&mut self.text_slots, self.text.text_paints(), updates);
        if split.is_empty() {
            return;
        }
        // Every measurement taken from a dropped paragraph goes with it. A box's cached size, its
        // baselines and the lines an inline formatting context resolved were all computed from
        // shaped runs that no longer exist, and a layout served from that cache asks for none of
        // them again — so the frame is laid out with no glyphs in it at all, and so is every frame
        // after it, because nothing will ever ask for a paragraph that a valid cache says it does
        // not need. Dropping the shaping and keeping the measurements is how a label disappears the
        // moment its colour stops moving.
        //
        // What is dropped is the elements' own, in both halves and in that order: the layout tree
        // is asked which contexts the named elements' text is in, and only the paragraphs it names
        // are thrown away. One control changing colour is one control's shaping, and answering it
        // with the window's costs a whole-document reflow for a change nothing else can see.
        let mut layout = self.layout.borrow_mut();
        let mut reshape = zgui_layout::text::reshape::scope(
            &mut layout,
            split.into_iter().map(|(node, _run)| node),
        );
        // A shaping key may be shared by an unaffected context. The changed context no longer
        // names its old key after `scope`, but deleting an entry another resolution still names
        // would leave that paragraph pointing at nothing.
        reshape
            .paragraphs
            .retain(|key| !layout.paragraph_is_active(*key));
        drop(layout);
        let dropped = self.text.forget_paragraphs(&reshape.paragraphs);
        zgui_profile::latency::note_with("t.reshaped", || format!("{dropped}/{}", reshape.boxes));
    }

    /// Copies the brushes into the display list, which is where the emitter reads them.
    ///
    /// Two tables exist because two things need one at times when they cannot share a borrow: the
    /// text engine claims slots while layout measures, and the display list is read while the emit
    /// walk runs. Reconciling them is therefore somebody's job, and this is the one place that
    /// holds both.
    ///
    /// It runs **after layout**, not beside [`Window::update_text_brushes`]. A slot is claimed the
    /// first time a paragraph is flattened, which is inside the layout pass — so a copy taken
    /// before it would be one slot short for every string that appeared this frame, and every one
    /// of them would be drawn in its element's own `color` for exactly one frame. That is a defect
    /// with no symptom at all in a document whose text colour is already the inherited one.
    fn publish_text_brushes(&mut self) {
        let table = self.text.text_paints();
        // A frame that moved no colour and flattened no new paragraph copies nothing: the slot
        // count is what every claim moves, and the brushes themselves are only ever rewritten by
        // the step above.
        if table.len() == self.scene.text_paints.len() && !self.brushes_moved {
            return;
        }
        self.brushes_moved = false;
        self.scene.text_paints = table.clone();
    }

    /// Brings the box tree back into agreement with the document, patching it where it can.
    ///
    /// A rebuild from the root is the last resort and not the default. It replaces every box, and a
    /// box's name is what fragment reuse, geometry diffing, the per-fragment paint record and
    /// damage scissoring are all keyed on — so a frame that rebuilds is a frame in which none of
    /// them can hit, every fragment compares as changed, and the damage collapses to the root's
    /// ink, which is the whole window.
    ///
    /// So the question asked here is *which elements* owe a rebuild rather than *whether any does*,
    /// and the two are not the same question: the obligation propagates to the root, so the root
    /// answers yes for a change to one element three panels away. The order is therefore: splice
    /// each marked element's own boxes in where its old ones were; rewrite the runs whose
    /// characters moved; and build the whole tree only when the splice reports a change it cannot
    /// confine or the rewrite reports one it cannot express.
    ///
    /// The structural obligations are retired here, and they have to be: this stage is their only
    /// consumer in the frame, so nothing else would ever clear them, and left set they are set for
    /// ever — which is the whole-window repaint above, on every frame, for the life of the window.
    /// Re-shaping is deliberately *not* retired, because the fragment pass reads it to decide that
    /// a line holding different glyphs must be painted again where it stands.
    pub(crate) fn build_boxes(&mut self) {
        let mut document = self.document.borrow_mut();
        let Some(root) = document.root_index() else {
            return;
        };
        // Retired whichever path is taken below, because every one of them services the whole set:
        // a splice services it where it was marked, and a build services it everywhere at once.
        let owed = zgui_layout::boxtree::retire(&mut document, root);
        let mut layout = self.layout.borrow_mut();
        let mut rebuild = layout.root().is_none();
        let mut spliced = None;
        if !rebuild && !owed.is_empty() {
            match zgui_layout::boxtree::patch::rebuild(&mut layout, &document, &owed) {
                Some(done) => spliced = Some(done),
                None => rebuild = true,
            }
        }
        // Text is rewritten even when a subtree was spliced: a frame that mounted a panel may also
        // have changed a label somewhere else, and the splice knows nothing about the label.
        if !rebuild {
            rebuild = zgui_layout::boxtree::patch::retext(&mut layout, &document, root)
                == zgui_layout::boxtree::patch::Retext::Rebuild;
        }
        zgui_profile::latency::note_with("b.why", || {
            format!(
                "owed={} spliced={spliced:?} noroot={} rebuild={rebuild}",
                owed.len(),
                layout.root().is_none()
            )
        });
        if rebuild {
            zgui_layout::boxtree::build(&mut layout, &document);
        }
    }

    /// Measures and arranges, then composes fragments and diffs them against the last frame.
    ///
    /// The measuring half is skipped when the boxes are already holding the answer it would
    /// produce, which is most frames: a colour that moved, a caret that blinked and an animation
    /// that only repaints all leave every box where the previous pass put it, and running the pass
    /// anyway walks the whole document to reach caches that all hit.
    ///
    /// The composing half is **not** skipped with it, and must not be. A frame that laid nothing
    /// out can still owe a repaint — that is precisely what a restyle which changed only paint is —
    /// and the damage for it is collected by the fragment pass. Skipping that pass on a held frame
    /// paints nothing at all for every change that does not move a box.
    pub(crate) fn lay_out(&mut self) {
        let device = DeviceStyle {
            scale: self.scale,
            ..DeviceStyle::default()
        };
        // Device pixels, not CSS ones. Every absolute length in a style has already been
        // multiplied by the scale by the time the tree reads it, so the extent it is measured
        // against has to be in the same units — and the surface's own extent is the only thing
        // that *is* the surface. Handing it the CSS extent laid a document out at one over the
        // scale of the surface it was drawn into, leaving the right and bottom of every
        // fractionally scaled window undrawn.
        let surface = self.surface.size();
        let width = surface.width.0;
        let height = surface.height.0;
        let relaid_out = {
            let mut layout = self.layout.borrow_mut();
            let mut tree = LayoutTree::new(&mut layout, &mut self.text, device);
            if let Some(custom) = self.custom_layout.as_deref() {
                tree = tree.with_custom(custom);
            }
            let outcome = tree.relayout_root(zgui_layout::tree::viewport_of(surface));
            if !outcome.had_a_root() {
                return;
            }
            outcome.ran()
        };
        // Between the two halves, and it can be nowhere else. A held offset is clamped against an
        // extent this pass has just recomputed, and the pass below composes every fragment against
        // whatever the offsets say — so an offset left past the end here is a subtree translated
        // clear of its own scrollport, which for the document's own scroll container is a window
        // with nothing in it. Skipped entirely when no pass ran, because then no extent moved.
        if relaid_out {
            self.clamp_scroll_to_content();
        }
        zgui_profile::latency::note_with("w.laidout", || {
            format!(
                "asked={width}x{height} scale={} surface={}x{} ran={relaid_out}",
                self.scale, surface.width.0, surface.height.0
            )
        });
        zgui_profile::latency::mark("f.fragments");

        let mut layout = self.layout.borrow_mut();
        let Some(root) = layout.root() else {
            return;
        };
        let mut document = self.document.borrow_mut();
        let scroll = self.scroll.borrow();
        let mut tables = zgui_layout::fragment::build::Tables {
            clips: &mut self.scene.clips,
            spatial: &mut self.scene.spatial,
            device,
            scroll: scroll.composed(),
            // Written by this frame's own animation tick, which ran before the restyle. An element
            // whose transform is being animated is composed against this rather than against its
            // style, and the two agree about everything else — see
            // [`Tier::Place`](zgui_anim::Tier::Place).
            placements: self.animator.placements(),
        };
        self.a11y_moves.clear();
        let mut marks = zgui_layout::fragment::diff::DocumentMarks::for_document(&mut document)
            .recording_moves(&mut self.a11y_moves);
        // Before the first walk of the frame and no other: what stands here on a later pass is
        // mostly the earlier pass's own movement, which is the one thing this must not collect.
        if self.layout_passes == 0 {
            self.damage_before_layout = self.damage;
        }
        self.layout_passes += 1;
        let moved = zgui_layout::fragment::diff::rebuild(
            &mut layout,
            &mut self.hit,
            &mut tables,
            &mut marks,
            root,
            &mut self.damage,
        );
        // Merged rather than assigned: a scroll delivered to a listener can re-render and relay
        // out inside the frame that delivered it, so the frame's answer is every pass's together.
        // The walk seeds its own `beyond` from whatever the frame had already damaged.
        self.rigid_moves = self.rigid_moves.and(moved);
        layout.reclaim_paragraphs();
        // The fragments name their matrices by an index into the table that was just filled, so
        // the two go to the view layer together: a box's place on the screen is only answerable
        // from both, and a view asking between frames must not be answered from one of them and
        // the other frame's other half.
        //
        // Which coordinate systems *moved* is asked for only when something outside this process
        // is holding a rectangle measured through one. Everything inside reads a matrix when it
        // wants one; only what has already been sent can be left describing where a box used to be,
        // and until a frame has sent one there is nothing to correct. This is the same test the
        // accessibility tree itself is built behind, for the same reason.
        self.moved_spaces.clear();
        // And only when the tree was written to at all: a document that is not animating,
        // not scrolling something sticky and not being restructured re-establishes every
        // coordinate system it had with the matrix it had, and nothing can have moved.
        let watching = self.a11y.held() > 0 && self.scene.spatial.written_since_recycle();
        self.host.publish_placements(
            &self.scene.spatial,
            watching.then_some(&mut self.moved_spaces),
        );
    }

    /// Re-tests what is under a pointer that has not moved.
    ///
    /// Content moves under a stationary cursor: a dropdown opens under it, a row scrolls out from
    /// under it. Hover is otherwise derived only from pointer events, so neither would be noticed
    /// until the pointer moved again. Bounded to one pass, and the state it writes is read by the
    /// *next* frame's restyle, so it cannot loop.
    fn rehit(&mut self) {
        let moved = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let filter = self.engine.filter();
            let world = zgui_input::World {
                document: &document,
                layout: &layout,
                hit: &self.hit,
                clips: &self.scene.clips,
                spatial: &self.scene.spatial,
                scale: zgui_geom::Scale::new(self.scale),
                filter: &filter,
            };
            self.router.rehit(&world)
        };
        // A boundary crossed by content moving is as real as one crossed by a pointer moving, so
        // the elements it changed are told the same way. Recorded rather than announced: this runs
        // in the middle of a frame, past the point where a handler's own consequences are carried
        // out, so the events go out at the top of the next one.
        if !moved.is_empty() {
            let pointer = self.pointer_now();
            self.note_crossings(&moved, pointer);
        }
    }

    /// Grows the damage over what reads outside itself, emits against it, and draws.
    ///
    /// The two halves of the content cache bracket the emit walk and both are load-bearing.
    /// [`ContentCache::begin_frame`](zgui_paint::ContentCache::begin_frame) before it is what makes
    /// eviction able to tell a glyph this frame drew from one it did not; the flush after it is
    /// what puts the texels on the device, because tiles are allocated as the walk reaches them
    /// and uploaded in one batch. Drawing before the flush samples texels that were never written,
    /// which is not a blank glyph but whichever glyph occupied the rectangle before.
    fn paint_and_draw(&mut self) -> FrameOutcome {
        use zgui_profile::latency::mark;
        let vector_before = self.renderer.vector_status();
        let size = self.surface.size();
        let viewport = Size::new(size.width.0 as i32, size.height.0 as i32);
        self.content.begin_frame();
        // Every fragment destroyed since the last painted frame, drained here — after all of this
        // frame's layout passes, before the emit walk — so the paint cache's records die exactly
        // when their fragments do. The release itself waits for the content borrow below.
        let retired = self.layout.borrow_mut().drain_retired_paint();
        // Read before the sink is borrowed, because what the device can do changes what is
        // *emitted* and not only how it is drawn.
        let capabilities = self.renderer.capabilities();
        // Decided inside the borrow below, performed after it: the copy belongs in the renderer's
        // own frame, and the renderer is not reachable while the layout store is borrowed.
        let shift: Option<zgui_render::ScrollShift>;
        let vector_report;
        {
            let layout = self.layout.borrow();
            // Before anything reads the set. What a scroll absorbs is where every moved fragment
            // was and is, which for a document taller than its window reaches far past the surface
            // on both sides; the renderer would cut that to the surface anyway, and until it is
            // cut the emit walk's subtree skip cannot refuse anything, so a scroll of one screenful
            // walks and paints the whole document.
            // Before the damage is read, and before it is grown: a scroll that can be answered by
            // moving pixels the renderer already has replaces the whole port with the bands the
            // move leaves undefined, and everything the frame damaged for any *other* reason is
            // kept and drawn over them.
            shift = self.scroll_shift(viewport).ok();
            if let Some(shift) = shift {
                // Everything this frame owes that the copy does not answer: what it inherited
                // before any layout pass, what the passes damaged beyond their movement, and the
                // bands the copy uncovers. What is dropped is exactly the movement's own damage,
                // which is the whole port and is what the copy is for.
                self.damage = self.damage_before_layout;
                self.damage.absorb_set(&self.rigid_moves.beyond);
                shift.expose_into(&mut self.damage);
            }
            self.damage
                .clip_to(zgui_geom::Rect::new(zgui_geom::Point::new(0, 0), viewport));
            mark("p.expand");
            zgui_paint::expand(&layout, &mut self.damage, viewport, self.scale);
            self.scene.begin_frame(viewport);
            // After the log has been emptied and before anything is pushed into it, which is the
            // only moment the names can begin being kept. A window that was never asked for them
            // pays a bool and two clears of empty vectors.
            self.scene
                .record_spatial_dependencies(self.check_spatial_dependencies);
            // No renderer is borrowed here, and none is reachable from anything the walk touches.
            // Rasterising a glyph, giving it a tile and growing the atlas to hold it are decided
            // against this window's own state; what a device is asked to do about any of it is
            // queued and performed at the flush below.
            let content = self
                .content
                .frame(&layout, &self.text, self.raster.as_ref());
            if !retired.is_empty() {
                self.painter.retire(&retired, &content);
            }
            // The device pixel ratio and what the device can do are both properties of this
            // window, and both change what is emitted: the first decides where a snapped edge
            // lands, the second whether text may be antialiased per colour channel. Emitting at a
            // fixed ratio while the damage above was grown at the real one lets the two disagree
            // on every display that is not exactly one device pixel per CSS pixel.
            // The document is read for one thing only: what each animating node's own override
            // currently holds. It is not folded into the lowered styles, because those are shared
            // between every element that cascaded to the same result and an animated value is not.
            let document = self.document.borrow();
            // The drawings are read through the same document, and placed into their boxes by a
            // cache of their own: a drawing is path notation on an element, and both the parse and
            // the fit onto the box are per-frame work a rasteriser would otherwise pay for by
            // re-encoding every icon on the screen every frame.
            let vectors = self.vectors.frame(&document);
            // The drawn frame's matrices, for reading a line's caret and selection marks in the
            // space the damage is measured in. They are the matrices of the frame on the screen
            // rather than the one being composed, which is the same reading the damage rectangles
            // for those marks were absorbed under.
            let placements = self.host.placements();
            let input = PaintInput {
                scale: self.scale,
                capabilities,
                glyphs: &content,
                glyph_placements: &content,
                placements: Some(&placements),
                highlights: self.carets.plan(),
                replaced: &content,
                vectors: &vectors,
                vector_masks: &content,
                custom: self
                    .custom_paint
                    .as_deref()
                    .unwrap_or(&zgui_paint::content::custom::NoCustom),
                // The same object again, and deliberately: what it answers here is not "give me
                // this" but "keep this". A range this frame records is replayed by later frames
                // without any of them looking a glyph up, so the record is what tells the atlas
                // those tiles are still on the screen.
                resources: &content,
                verify_replays: self.verify_replays,
                anim: &*document,
                // The bars are chrome rather than content, so nothing in the document cascades to
                // them; what they follow is the scheme the window is presented in, which is the
                // same input the sheet's own dark rules are matched against.
                scrollbars: zgui_paint::emit::scrollbar::paint_for(
                    self.scheme == zgui_style::ColorScheme::Dark,
                ),
                ..PaintInput::new(&layout, &self.damage)
            };
            zgui_profile::latency::note_with("d.postexpand", || {
                format!(
                    "full={} n={} area={:?} rects={:?}",
                    self.damage.is_full(),
                    self.damage.len(),
                    self.damage.area(),
                    self.damage.rects()
                )
            });
            mark("p.emit");
            let before = glyph_counts();
            let report = self.painter.emit(&input, &mut self.scene);
            vector_report = report.vector_routes;
            let after = glyph_counts();
            zgui_profile::latency::note_with("p.finish", || {
                format!(
                    "raster={} placed={} prims={} skipped={}",
                    after.1 - before.1,
                    after.0 - before.0,
                    report.primitives,
                    report.skipped_subtrees
                )
            });
        }
        let mut touched = rustc_hash::FxHashMap::default();
        for report in vector_report {
            touched
                .entry(report.node)
                .or_insert(zgui_paint::VectorRoutes::NONE)
                .union_with(report.routes);
        }
        let complex_this_frame: Vec<_> = touched
            .iter()
            .filter_map(|(node, routes)| {
                routes
                    .contains(zgui_paint::VectorRoute::GeneralRaster)
                    .then_some(*node)
            })
            .collect();
        for (node, routes) in touched {
            if routes.is_empty() {
                self.vector_routes.remove(&node);
            } else {
                self.vector_routes.insert(node, routes);
            }
        }
        // A node can stop being vector content altogether. Such a fragment produces no route-less
        // vector report — it is now an ordinary box — so retire retained diagnostics against the
        // current fragment kinds when the document changes. The revision gate matters for an icon-
        // heavy static or animating document: it keeps this from becoming a second per-frame walk
        // over every vector node solely for an inspector that may not be open.
        let vector_revision = self.dom.revision();
        if self.vector_routes_revision != vector_revision {
            let layout = self.layout.borrow();
            self.vector_routes.retain(|node, _| {
                layout.boxes_of(*node).iter().any(|box_| {
                    layout.fragments_of_box(*box_).iter().any(|fragment| {
                        layout.fragment(*fragment).is_some_and(|fragment| {
                            matches!(
                                fragment.kind,
                                zgui_layout::FragmentKind::Vector
                                    | zgui_layout::FragmentKind::Custom
                            )
                        })
                    })
                })
            });
            self.vector_routes_revision = vector_revision;
        }
        // A drawing placed for an element that has since gone would otherwise be held for the life
        // of the window, which for a list that scrolled through a thousand icons is a thousand
        // path allocations nothing can ever draw again.
        {
            let document = self.document.borrow();
            self.vectors
                .retain(|node| document.store().index_of(node).is_some());
            // The image loader owes the same frame-end truth-telling: a node that left takes its
            // attachment and its intrinsic claim with it, and a source nothing shows any more is
            // only a cache entry, which the budget — not this — decides the fate of.
            self.images.retain(
                |node| document.store().index_of(node).is_some(),
                &mut self.content,
            );
        }
        // Before the scene is finished, which is where a sprite still carrying a name rather than a
        // placement is refused: the arrays are sorted there, and a placeholder has no texture to be
        // sorted by. Costs an empty-list check on a frame every one of whose rasters was placed as
        // it was reached, which is every frame that rasterises as it walks.
        self.scene.resolve_resources(self.content.registry());
        self.scene.finish(&self.damage);
        // Before the renderer indexes a dense array of matrices with the numbers this list carries.
        // A primitive whose coordinate system changed hands under it draws a real box's content
        // through an unrelated box's matrix, and every other check the project has agrees with the
        // result: the bytes did not move, the geometry did not move, and the pixels are plausible.
        self.scene.check_spatial_dependencies();
        mark("p.upload");
        match self.content.flush(self.renderer.texture_sink()) {
            // Every queued write reached the device, so a shared attachment's tile can stand in
            // for its host texels from here on: the cache gives its copies back, and the loader
            // gives back its own for every tile the cache vouches is resident. Small pictures
            // keep theirs — the constant says why.
            Ok(_) => {
                self.content
                    .settle_uploaded(crate::images::RETAIN_SMALL_BYTES);
                let content = &self.content;
                self.images
                    .release_uploaded(|handle| content.image_tile_resident(handle));
            }
            // A refused upload is not a reason to drop the frame: what did reach the device still
            // draws, and the tiles that did not stay queued for the next one — and every host
            // copy is kept, which is the safe direction.
            Err(error) => {
                tracing::warn!(target: "zgui::paint", %error, "an atlas upload was refused");
            }
        }
        // Uploaded pictures whose tiles this frame found gone — evicted, or lost with the device
        // — have no host copy to re-upload from; the loader decodes them again from their
        // sources, and the completion's wake brings the frame that shows them.
        let missing = self.content.take_missing_images();
        if !missing.is_empty() && self.images.redecode_missing(&missing) {
            self.request_frame();
        }
        mark("p.budget");
        // After the walk and after the flush, and it can be nowhere else. Before the walk it would
        // be measured against the previous frame's working set; before the flush it would discard
        // the uploads this frame is about to draw from. Nothing at all happens to a cache that
        // stated no level to come back under.
        self.enforce_budgets();

        if self.occluded {
            // Nothing is acquired and nothing is presented, and the frame still ran: a callback
            // fired and a sleeping task resumed behind a minimised window.
            return FrameOutcome::Skipped(SkipReason::Occluded);
        }
        zgui_profile::latency::note_with("p.draw", || {
            format!("{}x{}", viewport.width, viewport.height)
        });
        // Handed over before the draw and after the occlusion check, because a window that is not
        // going to draw is not going to have anything to move either — and a shift recorded now and
        // drawn against a later frame would move pixels a different scroll offset already moved.
        if let Some(shift) = shift {
            self.renderer.shift_composed(shift);
        }
        let outcome = self.renderer.draw(&self.scene, &self.damage);
        let vector_after = self.renderer.vector_status();
        if !vector_before.initialized
            && vector_after.initialized
            && vector_after.backend == Some(zgui_render::VectorBackend::Vello)
        {
            self.vello_initializers = complex_this_frame;
        }
        outcome
    }
}

/// How many glyphs have been placed and how many of those had to be rasterised.
///
/// Read at both ends of the emit walk and subtracted, which is what makes the difference one
/// frame's own. Both read zero in a build compiled without the counters.
fn glyph_counts() -> (u64, u64) {
    (
        counter::get(Counter::GlyphsPlaced),
        counter::get(Counter::GlyphsRasterised),
    )
}
