//! Scrolling: the one write in the frame that is allowed to skip everything above the fragments.
//!
//! A scroll offset is state, not layout. Writing one marks `SCROLL` on the container that moved and
//! nothing else, and that single bit is what makes the fragment pass descend through it and compose
//! its descendants against the new offset. Nothing before that pass runs: no selector is matched, no
//! box is measured, no cached paint operation is produced again. The property is asserted rather
//! than hoped for — `scroll_does_not_restyle_relayout_or_reencode` reads the frame counters after a
//! wheel event over a real document — because it is the kind of property that decays silently: one
//! extra bit marked here would restyle a thousand rows per notch and look, on screen, exactly the
//! same.
//!
//! How far a device's scroll actually asked to go is a separate question with three owners, and it
//! is answered in [`asked`](self) rather than here. What a held offset owes when the window is
//! resized or moved to another output — the one thing that changes a scroll position without
//! anything having scrolled — is answered in [`extent`](self).
//!
//! `A11Y` rides along on the same container, and only on it. A scrolled list moves five thousand
//! nodes on the screen and changes exactly one thing the accessibility tree cares about — the
//! container's own scroll position — because every descendant's bounds are published relative to it.

mod asked;
mod bar;
mod extent;
mod frozen;
pub(crate) mod shift;

use zgui_dom::NodeKey;
use zgui_geom::{CssPx, Point, Scale};
use zgui_scroll::{Align, Behavior, Stretch};
use zgui_vocab::{EventKind, Payload, ScrollDelta, ScrollPhase, Timestamp};

use crate::window::Window;

/// How many refresh intervals may separate two frames before the gap stops being a slow frame.
///
/// Below it, a window is drawing late and a motion has to be where the clock puts it. At it and
/// above, what happened is a park — nothing was moving, so nothing was owed a frame, and the window
/// waited — and the gap describes time no motion existed for.
///
/// The same bound the animation deadline draws, for the same reason: see
/// [`AnimationCadence`](crate::AnimationCadence).
const PARKED_FOR: u32 = 8;

impl Window {
    /// Whether anything is scrolling on its own, so that the next frame has a deadline.
    pub(crate) fn is_scrolling(&self) -> bool {
        self.scroll.borrow().is_animating()
    }

    /// Scrolls whatever the wheel was over, and its ancestors when it runs out.
    ///
    /// The chain is why a bottomed-out list lets the page under it move: each container takes what
    /// it has room for and hands the rest outwards, and what the outermost cannot take displaces it
    /// past its end elastically.
    ///
    /// Three separate questions are answered here and each has a different owner. *Which* container
    /// scrolls is the document's; *how far* one detent asks to go is the desktop's, through
    /// [`ScrollSettings`](zgui_platform::ScrollSettings), because three lines to a detent is what
    /// the applications beside this one move; and *how tall a line is* is the scrolled container's
    /// own. A constant standing in for any one of them is a wheel that feels wrong somewhere and is
    /// never obviously broken anywhere.
    pub(crate) fn scroll_by(&mut self, container: NodeKey, delta: ScrollDelta, phase: ScrollPhase) {
        let chain = self.scroll_chain(container);
        let Some(&innermost) = chain.first() else {
            return;
        };
        let units = self.scroll_units(innermost);
        let asked = self.asked_for(delta);
        let moved_by =
            zgui_input::normalize::scroll::to_device(asked, units, Scale::new(self.scale));
        // A detent arrived whole and has to be carried; a continuous surface's deltas are already
        // a motion, and a second animation over them is what makes a trackpad feel like treacle.
        let settings = self.scroll_settings();
        let discrete = phase == ScrollPhase::Discrete;
        let travels = discrete && settings.wheel.framework_animates();
        // Whether the end of the content may be pulled past. A detent has no gesture behind it to
        // follow, so an edge that springs there bounces once per click at an end the person has
        // already arrived at and is only pushing against.
        let stretch = if if discrete {
            settings.elastic.admits_a_detent()
        } else {
            settings.elastic.admits_a_gesture()
        } {
            Stretch::Permitted
        } else {
            Stretch::Refused
        };
        let touched = {
            let layout = self.layout.borrow();
            let mut scroller = self.scroll.borrow_mut();
            if travels {
                scroller.glide_by(&layout, &chain, moved_by, stretch)
            } else {
                scroller.scroll_by(&layout, &chain, moved_by, stretch)
            }
        };
        // A glide's own movement is reported by the frames that carry it, but the frame it was
        // asked in still has to be one of them, or the wheel does nothing until something else
        // happens to wake the loop.
        self.mark_scrolled(&touched);
        if travels {
            self.request_scroll_frame();
        }
    }

    /// Asks for the frame that shows a motion this call started.
    fn request_scroll_frame(&mut self) {
        if !self.gate.request() {
            self.request_frame();
        }
    }

    /// Carries out a scroll a view asked for.
    pub(crate) fn carry_out_scroll(
        &mut self,
        node: zgui_view::NodeId,
        target: zgui_view::ScrollTarget,
        behavior: zgui_view::ScrollBehavior,
    ) {
        let Some(key) = zgui_view_dom::id::to_document(node) else {
            return;
        };
        let Some((container, to)) = self.destination(key, target) else {
            return;
        };
        self.place(container, to, translate(behavior));
    }

    /// Carries out what a touch gesture asked for.
    ///
    /// A finger dragging a list is a scroll and nothing else here: the content follows the contact,
    /// so the offset moves *against* the gesture, and lifting the finger hands the container the
    /// speed it left with. Everything else the recogniser reads — a tap, a long press, a pinch — is
    /// a reading a component acts on rather than a scroll, and is delivered rather than performed.
    ///
    /// The container a drag acts on is decided when the drag begins and not looked at again, so a
    /// finger that carries a list past the edge of its own scrollport keeps carrying that list.
    /// One drag at a time: a second contact that begins panning takes the scroll over, which is
    /// what a second finger landing on a list is asking for.
    pub(crate) fn carry_out_gestures(&mut self, read: &[zgui_input::Gesture]) {
        for gesture in read {
            match gesture {
                zgui_input::Gesture::PanStart { from, .. } => {
                    // Decided once, where the finger went down, and held for the whole drag. A
                    // container re-derived from where the finger is *now* is a list that stops
                    // following it the moment the drag leaves the scrollport — which is most
                    // drags, because the content moves under the contact and the contact does not
                    // stop at the edge.
                    self.panning = match self.node_at(*from) {
                        Some(node) => self.scroll_chain(node),
                        None => Vec::new(),
                    };
                }
                zgui_input::Gesture::PanMove { by, .. } => {
                    if self.panning.is_empty() {
                        continue;
                    }
                    let chain = core::mem::take(&mut self.panning);
                    let scale = self.scale;
                    let moved_by = zgui_geom::Size::new(
                        zgui_geom::DevicePx(-by.width.0 * scale),
                        zgui_geom::DevicePx(-by.height.0 * scale),
                    );
                    // A contact that is still down is the case the spring exists for, so this
                    // asks the gesture question rather than the detent one.
                    let stretch = if self.scroll_settings().elastic.admits_a_gesture() {
                        Stretch::Permitted
                    } else {
                        Stretch::Refused
                    };
                    let touched = {
                        let layout = self.layout.borrow();
                        self.scroll
                            .borrow_mut()
                            .scroll_by(&layout, &chain, moved_by, stretch)
                    };
                    self.panning = chain;
                    self.mark_scrolled(&touched);
                }
                zgui_input::Gesture::PanEnd { velocity, .. } => {
                    let container = self.panning.first().copied();
                    self.panning = Vec::new();
                    let Some(container) = container else {
                        continue;
                    };
                    let scale = self.scale;
                    let thrown = zgui_geom::Size::new(
                        zgui_geom::DevicePx(-velocity.x * scale),
                        zgui_geom::DevicePx(-velocity.y * scale),
                    );
                    let layout = self.layout.borrow();
                    self.scroll.borrow_mut().fling(&layout, container, thrown);
                }
                // A tap, a long press and a pinch are readings a component acts on. Performing one
                // here would be this layer deciding what a long press means, which is a decision
                // that belongs to whatever was pressed.
                _ => {}
            }
        }
    }

    /// Reports every contact that has now been held long enough to mean something else.
    ///
    /// A long press is the one reading of the touch stream that is produced by time passing rather
    /// than by anything arriving, so the frame has to ask. What it means is the platform's own
    /// convention and not an invention here: on a touch surface, pressing and holding is how a
    /// context menu is opened, and that is the event it is delivered as.
    pub(crate) fn advance_gestures(
        &mut self,
        wall: std::time::Instant,
        now: zgui_vocab::Timestamp,
    ) {
        if !self.gestures.awaits_deadline() {
            self.long_press_due = None;
            return;
        }
        let read = self.gestures.elapsed(now);
        // Recomputed from the recogniser after it has been given this frame's time, so a contact
        // that has just fired — or has just stopped being a candidate — leaves nothing installed.
        // The two clocks are reconciled here because this is the one place that holds both.
        self.long_press_due = self.gestures.next_deadline(now).map(|left| wall + left);
        for gesture in read {
            let zgui_input::Gesture::LongPress { at, .. } = gesture else {
                continue;
            };
            let Some(node) = self.node_at(at) else {
                continue;
            };
            self.synthesize(
                zgui_view_dom::id::to_view(node),
                zgui_vocab::EventKind::ContextMenu,
                now,
            );
        }
    }

    /// When a contact being held now becomes a long press, if one can.
    ///
    /// A held finger is **not** an animation, and answering it as one is the difference between a
    /// press-and-hold costing one wake and costing one wake per refresh interval for as long as it
    /// is held: nothing about the contact changes between the press and the half-second mark, so
    /// every frame in between draws the same pixels. This is the moment the recogniser actually
    /// owes an answer at, and it is the only wake a held contact is worth.
    pub(crate) fn gesture_deadline(&self) -> Option<std::time::Instant> {
        self.long_press_due
    }

    /// The innermost element under a point, in CSS pixels from the window's corner.
    fn node_at(&self, at: zgui_geom::Point<CssPx, zgui_geom::Css>) -> Option<NodeKey> {
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let filter = self.engine.filter();
        let world = zgui_input::World {
            document: &document,
            layout: &layout,
            hit: &self.hit,
            clips: &self.scene.clips,
            spatial: &self.scene.spatial,
            scale: Scale::new(self.scale),
            filter: &filter,
        };
        let point = zgui_geom::Point::new(
            zgui_geom::DevicePx(at.x.0 * self.scale),
            zgui_geom::DevicePx(at.y.0 * self.scale),
        );
        world.chain_at(point).target()
    }

    /// Brings one element into view inside whatever contains it.
    ///
    /// This is what an accessibility action, a focus move and a `scroll_into_view` from a view all
    /// ask for, so it is public rather than reached through the command queue: the caller here
    /// already holds the window and is not inside a handler.
    pub fn scroll_into_view(&mut self, node: NodeKey, align: Align, behavior: Behavior) {
        let Some((container, to)) = self.aligned_offset(node, align) else {
            return;
        };
        self.place(container, to, behavior);
    }

    /// Puts one container at an offset, and marks it when that changed anything.
    ///
    /// A scroll to where the container already is marks nothing. Marking it anyway would damage the
    /// scrollport and redraw it identically, once per call — and a component that keeps a selected
    /// row in view calls this on every keystroke, most of which move nothing.
    fn place(
        &mut self,
        container: NodeKey,
        to: Point<zgui_geom::DevicePx, zgui_geom::Device>,
        behavior: Behavior,
    ) {
        // A frozen window does not move for anything, including a view asking outright and an
        // accessibility action bringing something into view. Leaving those two ways in would make
        // the page jump under a modal surface exactly as restyling the root did, only sometimes.
        if self.is_frozen(container) {
            return;
        }
        let before = self.scroll.borrow().offset_of(container);
        let moved = {
            let layout = self.layout.borrow();
            self.scroll
                .borrow_mut()
                .scroll_to(&layout, container, to, behavior)
        };
        let changed = match moved {
            Some(_) if behavior.animates() => self.scroll.borrow().is_animating(),
            Some(_) => self.scroll.borrow().offset_of(container) != before,
            None => false,
        };
        if changed {
            self.mark_scrolled(&[container]);
        }
    }

    /// Advances every smooth scroll and every elastic edge, and marks what moved.
    ///
    /// A gap longer than [`PARKED_FOR`] intervals is spent on nothing rather than on the motion,
    /// and that is the whole of what separates a slow frame from a window that had stopped. The
    /// frame that starts a motion is the frame that drained the event which started it, and the
    /// frame before *that* one may be however long ago the window last had anything to do — so a
    /// gap taken at face value hands a motion the whole of a park on its first step. A wheel turned
    /// on a window that has been still for four seconds arrives at its destination in the frame it
    /// was turned in, and an edge dragged past its end springs all the way back before anything is
    /// drawn: the motion happens inside one frame, damages nothing, and is indistinguishable from a
    /// scroll that does not animate at all.
    ///
    /// Below the bound the gap is kept exactly, because there it is a frame that took too long and
    /// the content has to be where the clock says it is. Nothing is clamped or spread: a motion is
    /// a function of the time that passed, and a window under load shows fewer steps of it rather
    /// than a slower one.
    pub(crate) fn advance_scroll(&mut self, now: Timestamp) {
        let since = self
            .last_frame
            .map(|previous| now.saturating_since(previous))
            .unwrap_or_default();
        self.last_frame = Some(now);
        if !self.is_scrolling() {
            return;
        }
        let elapsed = if since > self.refresh_interval() * PARKED_FOR {
            core::time::Duration::ZERO
        } else {
            since
        };
        if elapsed.is_zero() {
            return;
        }
        let touched = {
            let layout = self.layout.borrow();
            self.scroll.borrow_mut().advance(&layout, elapsed)
        };
        self.mark_scrolled(&touched);
    }

    /// Dispatches a `scroll` event on every container that moved during this frame.
    ///
    /// It runs after the fragment pass and before anything is painted, so a handler that
    /// repositions something in response is drawn in its final place in the frame the scroll
    /// happened in rather than one frame behind it.
    ///
    /// A container that moved several times in one frame — a wheel event and a smooth scroll's tick
    /// — is reported once, from where it started to where it ended.
    pub(crate) fn dispatch_scroll(&mut self, timestamp: Timestamp) {
        let moved = self.scroll.borrow_mut().take_moved();
        if moved.is_empty() {
            return;
        }
        // Kept before the log is consumed. What the renderer may be asked to translate is decided
        // at paint time, which is well after this, and this is the only place the frame's own
        // movements are enumerated.
        self.scrolled_this_frame.extend_from_slice(&moved);
        let scale = Scale::new(self.scale);
        for scrolled in moved {
            let (node, payload) = {
                let layout = self.layout.borrow();
                let node = scrolled.container;
                let Some(event) =
                    zgui_scroll::report::event(&layout, scrolled.container, scrolled.to, scale)
                else {
                    continue;
                };
                (node, Payload::Scroll(event))
            };
            let steps = {
                let document = self.document.borrow();
                let chain = zgui_input::HitChain::to_root(document.store(), node);
                let mut plan = zgui_input::dispatch::Plan::default();
                zgui_input::dispatch::resolve(
                    document.store(),
                    &chain,
                    EventKind::Scroll,
                    &mut plan,
                );
                plan.steps().to_vec()
            };
            if steps.is_empty() {
                continue;
            }
            crate::dispatch::run_discarding(
                self,
                &steps,
                EventKind::Scroll,
                Some(node),
                &payload,
                zgui_vocab::Modifiers::NONE,
                timestamp,
            );
        }
        // A handler's writes have to settle in this frame, exactly as an observation delivery's do,
        // or a virtualised list renders the rows it had before the scroll for one frame.
        zgui_reactive::flush();
        self.carry_out_commands(timestamp);
        self.restyle_and_relayout_after_delivery();
    }

    /// The containers a wheel over `from` acts on, innermost first.
    ///
    /// The window's own container is left out of it while the window is frozen, which is what
    /// makes a wheel, a trackpad and a key all leave the page where it is behind a modal surface
    /// without anything about the page having been restyled. Everything else in the chain is
    /// untouched: a list inside the page still takes what is aimed at it.
    fn scroll_chain(&self, from: NodeKey) -> Vec<NodeKey> {
        let mut chain = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            zgui_input::dispatch::defaults::scroll_chain(document.store(), &layout, from).to_vec()
        };
        if let Some(frozen) = self.frozen_container() {
            chain.retain(|container| *container != frozen);
        }
        chain
    }

    /// Which container a `scroll_to` moves, and where to.
    fn destination(
        &self,
        node: NodeKey,
        target: zgui_view::ScrollTarget,
    ) -> Option<(NodeKey, Point<zgui_geom::DevicePx, zgui_geom::Device>)> {
        let align = match target {
            zgui_view::ScrollTarget::Offset(to) => {
                return Some((node, to));
            }
            zgui_view::ScrollTarget::By(delta) => {
                let container = node;
                let at = self.scroll.borrow().offset_of(container);
                return Some((
                    container,
                    Point::new(
                        zgui_geom::DevicePx(at.x.0 + delta.x.0),
                        zgui_geom::DevicePx(at.y.0 + delta.y.0),
                    ),
                ));
            }
            zgui_view::ScrollTarget::IntoView => Align::Nearest,
            zgui_view::ScrollTarget::IntoViewStart => Align::Start,
            zgui_view::ScrollTarget::IntoViewEnd => Align::End,
            zgui_view::ScrollTarget::IntoViewCenter => Align::Center,
            // A placement this build has never heard of moves as little as possible, which is what
            // every one of the named ones degrades to.
            _ => Align::Nearest,
        };
        self.aligned_offset(node, align)
    }

    /// Which container brings `node` into view, and the offset that does it.
    fn aligned_offset(
        &self,
        node: NodeKey,
        align: Align,
    ) -> Option<(NodeKey, Point<zgui_geom::DevicePx, zgui_geom::Device>)> {
        // Bringing a node into view scrolls the thing that contains it, not the node itself.
        let container = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let mut chain =
                zgui_input::dispatch::defaults::scroll_chain(document.store(), &layout, node)
                    .into_iter()
                    .filter(|found| *found != node);
            chain.next()?
        };
        let layout = self.layout.borrow();
        let target_box = *layout.boxes_of(node).first()?;
        let container_box = *layout.boxes_of(container).first()?;
        let target = layout.fragments_of_box(target_box).first().copied()?;
        let port = layout.fragments_of_box(container_box).first().copied()?;
        let target = layout.fragment(target)?.border_box;
        let port = layout.fragment(port)?.content_box;
        let at = self.scroll.borrow().offset_of(container);
        Some((
            container,
            zgui_scroll::into_view::offset_for(at, target, port, align),
        ))
    }

    /// Marks every container that moved, and asks for the frame that shows it.
    ///
    /// The request goes through both routes on purpose. A scroll raised inside a frame — a wheel
    /// event being drained, a motion advancing — is folded into the one request the frame's last
    /// phase makes; one raised from outside a frame, which is what an accessibility action and a
    /// `scroll_into_view` from an application both are, has nobody behind it and must ask the
    /// surface itself.
    fn mark_scrolled(&mut self, containers: &[NodeKey]) {
        if containers.is_empty() {
            return;
        }
        {
            let mut document = self.document.borrow_mut();
            zgui_scroll::mark::scrolled(&mut document, containers);
        }
        if !self.gate.request() {
            self.request_frame();
        }
    }
}

/// The scrolling system's name for a behaviour the view layer named.
///
/// Two enumerations rather than one because the view layer must not depend on the scrolling system,
/// and both are closed sets this one function keeps in step.
fn translate(behavior: zgui_view::ScrollBehavior) -> Behavior {
    match behavior {
        zgui_view::ScrollBehavior::Smooth => Behavior::Smooth,
        // Arriving at once is what every behaviour degrades to, including one this build has never
        // heard of: the content ends up where it was asked to be.
        _ => Behavior::Instant,
    }
}
