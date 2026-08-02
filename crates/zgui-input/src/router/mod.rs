//! One event in, three answers out.
//!
//! Everything in the modules above is a piece of routing one event: find what is under the
//! pointer, write what that changed about the document, and resolve who hears about it. This is
//! where they are put together, and it exists so that they are put together the same way every
//! time — the order of those steps is not free. Interaction state is written *before* listeners
//! are resolved, so that a handler reading a computed style sees the state its own event caused;
//! and the framework's default behaviour is *computed* here and carried out by the caller, after
//! every listener has had the chance to say that it should not be.

mod bars;
mod world;

use smallvec::SmallVec;
use zgui_vocab::{
    EventKind, KeyEvent, Modifiers, Payload, PointerAction, PointerEvent, Timestamp, WheelEvent,
};

use crate::capture::PointerCapture;
use crate::dispatch::{FrameworkDefault, Plan, Step, defaults, resolve};
use crate::hit::HitChain;
use crate::normalize::pointer::Pointers;
use crate::router::bars::{Bar, Bars};
use crate::state::Interaction;
use crate::state::focus::FocusSource;
use crate::state::within::Moved;

pub use crate::router::world::World;

/// What one event turned out to mean.
///
/// The listeners are borrowed from the router, which is what keeps one buffer for a whole run and
/// what stops a second event being routed while the first one's answer is still being walked.
#[derive(Debug)]
pub struct Routed<'a> {
    /// Which event this is.
    pub kind: EventKind,
    /// What it carries.
    pub payload: Payload,
    /// The path it travels, root first. Empty when it landed on nothing.
    pub chain: HitChain,
    /// Which listeners run, in the order they run in.
    pub steps: &'a [Step],
    /// What the framework does about it if no handler says otherwise.
    pub default: Option<FrameworkDefault>,
    /// Which elements gained or lost `:hover` because of it.
    pub hover: Moved,
    /// Which elements gained or lost `:active` because of it.
    pub active: Moved,
}

impl Routed<'_> {
    /// The element the event was aimed at.
    pub fn target(&self) -> Option<zgui_dom::NodeKey> {
        self.chain.target()
    }
}

/// The input system's own state: what is hovered, pressed, focused, captured and trapped.
///
/// One per window. It holds no document and no layout — those are handed to it per event, because
/// they belong to the frame and this outlives every frame.
#[derive(Debug, Default)]
pub struct Router {
    /// Where each pointer is.
    pointers: Pointers,
    /// Which element each pointer is captured by.
    capture: PointerCapture,
    /// Hover, press and focus.
    interaction: Interaction,
    /// The reusable resolved order.
    plan: Plan,
    /// Which element each pointer was pressed on, for deciding activation.
    pressed: SmallVec<[(zgui_vocab::PointerId, zgui_dom::NodeKey); 2]>,
    /// Which scrollbar each pointer is holding.
    bars: Bars,
}

impl Router {
    /// A router with nothing hovered, pressed or focused.
    pub fn new() -> Self {
        Self::default()
    }

    /// What the input system knows about how the document is being interacted with.
    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }

    /// The same, for a caller carrying out a focus move it was handed.
    pub fn interaction_mut(&mut self) -> &mut Interaction {
        &mut self.interaction
    }

    /// Which element each pointer is captured by.
    pub fn capture(&self) -> &PointerCapture {
        &self.capture
    }

    /// The same, for carrying out a handler's request to capture or release.
    pub fn capture_mut(&mut self) -> &mut PointerCapture {
        &mut self.capture
    }

    /// Where each pointer is.
    pub fn pointers(&self) -> &Pointers {
        &self.pointers
    }

    /// Routes one pointer event.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn pointer(
        &mut self,
        world: &World<'_>,
        action: PointerAction,
        event: &PointerEvent,
        modifiers: Modifiers,
        timestamp: Timestamp,
    ) -> Routed<'_> {
        let _ = (modifiers, timestamp);
        self.pointers.observe(action, event, world.scale);
        let chain = self.aim(world, action, event);

        // State first, listeners second: a handler that reads a computed style has to see the
        // state its own event produced, and a restyle driven from a bit written afterwards would
        // land a frame late.
        let hover = self.hover(world, action, &chain);
        // A scrollbar answers before anything else does, and takes the event away from the rest of
        // the framework's behaviour when it answers at all. It has to: a press on a bar belongs to
        // the element that scrolls, so left to the ordinary path it would take focus off whatever
        // the person was typing in and, on release, click the container it landed on.
        let bar = self.bars(world, action, event);
        // Before the press bookkeeping, not after: a release is an activation of whatever the
        // press landed on, and the press bookkeeping is what forgets that.
        let default = match bar {
            Bar::Took(default) => default,
            Bar::Untouched => self.pointer_default(world, action, event, &chain),
        };
        let active = self.press(world, action, event, &chain, bar);

        let kind = action.event_kind();
        resolve(world.document.store(), &chain, kind, &mut self.plan);
        Routed {
            kind,
            payload: Payload::Pointer(*event),
            chain,
            steps: self.plan.steps(),
            default,
            hover,
            active,
        }
    }

    /// Routes one wheel event, which is aimed where the pointer is and scrolls what is under it.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn wheel(&mut self, world: &World<'_>, event: &WheelEvent) -> Routed<'_> {
        let point = zgui_geom::Point::new(
            zgui_geom::DevicePx(event.position.x.0 * world.scale.get()),
            zgui_geom::DevicePx(event.position.y.0 * world.scale.get()),
        );
        let chain = world.chain_at(point);
        let default = defaults::on_wheel(
            world.document.store(),
            world.layout,
            &chain,
            event.delta,
            event.phase,
        );
        resolve(
            world.document.store(),
            &chain,
            EventKind::Wheel,
            &mut self.plan,
        );
        Routed {
            kind: EventKind::Wheel,
            payload: Payload::Wheel(*event),
            chain,
            steps: self.plan.steps(),
            default,
            hover: Moved::default(),
            active: Moved::default(),
        }
    }

    /// Routes one key event, which is aimed at whatever has focus.
    ///
    /// With nothing focused the path is the document's root and nothing else, so an ordinary
    /// listener anywhere below the root hears nothing whatever. `shortcuts` names the elements
    /// that asked to hear a key anyway; each one's own registrations are appended after the path's,
    /// and only its own. That is the whole of the difference between a window shortcut and a deep
    /// key handler: naming the elements that want the keyboard when nobody has it, rather than
    /// widening the path and handing an unfocused key to every list, editor and menu on the way
    /// down.
    ///
    /// A shortcut already on the path is not appended twice.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn key(
        &mut self,
        world: &World<'_>,
        state: zgui_vocab::KeyState,
        event: &KeyEvent,
        modifiers: Modifiers,
        shortcuts: &[zgui_dom::NodeKey],
    ) -> Routed<'_> {
        let focused = self.interaction.focus.focused();
        let chain = match focused {
            Some(node) => HitChain::to_root(world.document.store(), node),
            None => world.root_chain(),
        };
        let default = if state == zgui_vocab::KeyState::Pressed {
            defaults::on_key(event, modifiers, focused)
        } else {
            None
        };
        let kind = state.event_kind();
        resolve(world.document.store(), &chain, kind, &mut self.plan);
        // Only when nothing is focused. A window whose keyboard is inside a field routes the key
        // down to that field in the ordinary way, and a shortcut that fired as well would take the
        // chord away from every control that binds one of its own.
        if focused.is_none() {
            for node in shortcuts {
                if !chain.contains(*node) {
                    crate::dispatch::append(world.document.store(), *node, kind, &mut self.plan);
                }
            }
        }
        Routed {
            kind,
            payload: Payload::Key(event.clone()),
            chain,
            steps: self.plan.steps(),
            default,
            hover: Moved::default(),
            active: Moved::default(),
        }
    }

    /// Moves focus, as a default action or an application's own request asked.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn focus(
        &mut self,
        world: &World<'_>,
        node: Option<zgui_dom::NodeKey>,
        source: FocusSource,
    ) -> (Option<zgui_dom::NodeKey>, Option<zgui_dom::NodeKey>) {
        let chain = match node {
            Some(node) => HitChain::to_root(world.document.store(), node),
            None => HitChain::default(),
        };
        self.interaction
            .focus
            .move_to(world.document, world.filter, &chain, source)
    }

    /// Re-tests what is under each pointer without any pointer having moved, and rewrites hover.
    ///
    /// A frame that moved a fragment can move something out from under a stationary cursor, and
    /// the pointer that is now over something else will not say so until it is moved again. Run
    /// once per frame that changed geometry; it writes nothing when nothing moved.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn rehit(&mut self, world: &World<'_>) -> Moved {
        let Some((_, point)) = self.pointers.all().next() else {
            return Moved::default();
        };
        let chain = world.chain_at(point);
        self.interaction
            .hover
            .move_to(world.document, world.filter, &chain)
    }

    /// Lets go of whatever is being pressed, and of any pointer capture.
    ///
    /// What a window does when the release it is waiting for is never going to arrive: the surface
    /// loses the keyboard mid-drag, a grab elsewhere takes the pointer. `:active` is written by a
    /// press and cleared by the matching release, so a press whose release went somewhere else
    /// leaves a control lit up for as long as the window lives, and a control that captured the
    /// pointer keeps every event in the window pointed at itself.
    ///
    /// Two things are deliberately left alone.
    ///
    /// **Focus**, because the person is still in the field they were typing in: a window that
    /// dropped focus here would come back with the caret nowhere and the next key going to the
    /// document.
    ///
    /// **Hover**, because it is not stuck. A surface that has lost the keyboard still receives
    /// pointer motion — a window nobody is typing into still lights up under the cursor, on every
    /// desktop — and what is under the pointer is re-derived from where the pointer is, so clearing
    /// it here would be undone by the next frame that looked.
    ///
    /// Returns which elements changed state, for a caller that has to know whether anything moved.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn cancel_press(
        &mut self,
        document: &zgui_dom::Document,
        filter: &dyn zgui_dom::StyleFilter,
    ) -> Moved {
        let moved = self.interaction.active.release(document, filter);
        self.capture.clear();
        self.pressed.clear();
        self.bars.clear();
        moved
    }

    /// Forgets everything about an element that has gone.
    ///
    /// Without this, an element removed while it held the pointer keeps every later event aimed at
    /// a node that no longer exists.
    pub fn forget(&mut self, node: zgui_dom::NodeKey) {
        self.capture.release_node(node);
        self.pressed.retain(|(_, held)| *held != node);
        self.bars.forget(node);
    }

    /// Where this event is aimed: the capturing element if there is one, otherwise what is under
    /// the pointer.
    fn aim(&self, world: &World<'_>, action: PointerAction, event: &PointerEvent) -> HitChain {
        if matches!(action, PointerAction::Left) {
            return HitChain::default();
        }
        let under = world.chain_at(crate::normalize::pointer::device_position(
            event,
            world.scale,
        ));
        match self.capture.of(event.id) {
            // The capture holds even when the pointer is somewhere else entirely, which is the
            // whole point of it: a slider being dragged keeps receiving the pointer after it has
            // left the slider.
            Some(node) => match under.truncated_at(node) {
                held if !held.is_empty() => held,
                _ => HitChain::to_root(world.document.store(), node),
            },
            None => under,
        }
    }

    /// Applies what this action does to `:hover`.
    fn hover(&mut self, world: &World<'_>, action: PointerAction, chain: &HitChain) -> Moved {
        match action {
            PointerAction::Left | PointerAction::Cancelled => {
                self.interaction.hover.clear(world.document, world.filter)
            }
            _ => self
                .interaction
                .hover
                .move_to(world.document, world.filter, chain),
        }
    }

    /// What this event did to the scrollbars, and what they ask for in return.
    ///
    /// The drag is computed from where the pointer is now and the grab it started with, never from
    /// how far the pointer moved since the last event: a thumb that accumulated deltas would drift
    /// away from the pointer over a long drag, and would keep the drift after the content hit its
    /// end and stopped taking movement.
    fn bars(&mut self, world: &World<'_>, action: PointerAction, event: &PointerEvent) -> Bar {
        let point = crate::normalize::pointer::device_position(event, world.scale);
        match action {
            PointerAction::Pressed => match world.scrollbar_at(point) {
                Some(press) => {
                    let default = self.bars.press(event.id, &press);
                    // The bar keeps the pointer for the whole gesture, so a thumb dragged off its
                    // own track goes on being dragged and nothing the pointer passes over lights
                    // up on the way.
                    self.capture.set(event.id, press.container);
                    Bar::Took(default)
                }
                None => Bar::Untouched,
            },
            PointerAction::Moved => match self.bars.of(event.id) {
                // The bar keeps the move whatever comes of it: a container that has gone, or one
                // whose space has collapsed, scrolls nowhere — but the pointer is still holding its
                // bar, and letting the move fall through to the document would light up whatever it
                // is passing over in the middle of a drag.
                Some(held) => Bar::Took(
                    crate::hit::scrollbar::along_bar(
                        world.layout,
                        world.spatial,
                        held.container,
                        held.axis,
                        point,
                    )
                    .and_then(|at| bars::dragged(world.layout, &held, at)),
                ),
                None => Bar::Untouched,
            },
            PointerAction::Released | PointerAction::Cancelled => match self.bars.of(event.id) {
                Some(_) => {
                    self.bars.release(event.id);
                    self.capture.release(event.id);
                    Bar::Took(None)
                }
                None => Bar::Untouched,
            },
            _ => Bar::Untouched,
        }
    }

    /// Applies what this action does to `:active`, and remembers what was pressed.
    ///
    /// A press a scrollbar took writes neither: the bar is chrome rather than content, so the
    /// container it belongs to is not `:active` because its bar was pressed, and the release that
    /// ends the drag is not a click on it.
    fn press(
        &mut self,
        world: &World<'_>,
        action: PointerAction,
        event: &PointerEvent,
        chain: &HitChain,
        bar: Bar,
    ) -> Moved {
        if matches!(bar, Bar::Took(_)) {
            return match action {
                PointerAction::Pressed => self.interaction.active.press(
                    world.document,
                    world.filter,
                    &HitChain::default(),
                ),
                PointerAction::Released | PointerAction::Cancelled => self
                    .interaction
                    .active
                    .release(world.document, world.filter),
                _ => Moved::default(),
            };
        }
        match action {
            PointerAction::Pressed => {
                if let Some(target) = chain.target() {
                    match self.pressed.iter_mut().find(|(id, _)| *id == event.id) {
                        Some((_, held)) => *held = target,
                        None => self.pressed.push((event.id, target)),
                    }
                }
                self.interaction
                    .active
                    .press(world.document, world.filter, chain)
            }
            PointerAction::Released | PointerAction::Cancelled => {
                self.pressed.retain(|(id, _)| *id != event.id);
                self.interaction
                    .active
                    .release(world.document, world.filter)
            }
            _ => Moved::default(),
        }
    }

    /// What the framework would do about this action on its own account.
    fn pointer_default(
        &self,
        world: &World<'_>,
        action: PointerAction,
        event: &PointerEvent,
        chain: &HitChain,
    ) -> Option<FrameworkDefault> {
        match action {
            PointerAction::Pressed => Some(defaults::on_press(
                world.document.store(),
                Some(world.layout),
                chain,
            )),
            PointerAction::Released => defaults::on_release(
                chain,
                self.pressed
                    .iter()
                    .find(|(id, _)| *id == event.id)
                    .map(|(_, node)| *node),
            ),
            _ => None,
        }
    }
}
