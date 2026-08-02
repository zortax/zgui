//! Publishing the accessibility tree, and carrying out what comes back through it.
//!
//! # Why the marks are drained on every frame and the tree is built on almost none
//!
//! Building the tree costs a walk of everything that changed. On a machine with no assistive
//! technology running — most machines, most of the time — that walk is pure waste, so the surface
//! is handed a *closure* and calls it only when something is listening.
//!
//! Draining the document's accessibility marks is a different question and has the opposite answer.
//! The invalidation lattice is a union: a bit left set on a node keeps every ancestor's subtree
//! union set, and a phase nothing ever retires makes every other stage descend everywhere for the
//! life of the window. So the marks are drained on every frame whether anything is listening or
//! not, and what they gather accumulates until a build consumes it.
//!
//! # Why an action is dispatched and not carried out directly
//!
//! An inbound `Action::Click` becomes a synthesised `click` on the target, down the same capture,
//! target and bubble path a pointer produces. That is the whole of activation: a component that
//! responds to being clicked responds to a screen reader activating it, with nothing written twice
//! and nothing to forget.

use zgui_a11y::{ActionRequest, Intent, World};
use zgui_vocab::{EventKind, Payload, Timestamp, ValueChange, ValueEvent};

use crate::window::Window;

impl Window {
    /// Drains the document's accessibility marks and publishes what they came to.
    ///
    /// The last phase of the frame that has anything to say to a consumer, and it runs after the
    /// renderer: what a consumer is told about a node's position must be what was drawn, not what
    /// was about to be.
    pub(crate) fn publish_a11y(&mut self) {
        for node in self.a11y_moves.drain(..) {
            self.a11y.note_move(node);
        }
        // The same obligation, raised by the thing that outlives the walk which raises the other
        // one. A node the fragment pass carried somewhere else is reported by name because the pass
        // touched it; a node whose coordinate system was written to was not touched at all, and the
        // only record that it is now drawn elsewhere is the matrix the name resolves to.
        for space in self.moved_spaces.drain(..) {
            self.a11y.note_space_moved(space);
        }
        {
            let mut document = self.document.borrow_mut();
            self.a11y.collect(&mut document);
        }
        match (self.a11y.is_owed(), self.focus_moved_since_publish()) {
            (false, false) => {}
            // Nothing about the document changed and focus did: an update carrying no nodes at all
            // is the whole of what there is to say, and building one costs no projection.
            (false, true) => self.push_focus_only(),
            (true, _) => self.push_a11y(false),
        }
    }

    /// Publishes a whole tree, for a consumer that has just connected and holds nothing.
    ///
    /// A request for the initial tree cannot be answered from a dirty check: nothing is dirty,
    /// because nothing has changed — what is missing is the consumer's copy.
    pub fn publish_full_a11y_tree(&mut self) {
        self.a11y.forget();
        self.push_a11y(true);
    }

    /// Builds and hands over one update, building it only if something is listening.
    fn push_a11y(&mut self, full: bool) {
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let placements = self.host.placements();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: self.scale,
            focus: self.router.interaction().focus.focused(),
        };
        let builder = &mut self.a11y;
        self.surface.push_a11y_update(&mut || {
            let update = if full {
                builder.build_full(&world)
            } else {
                builder.build(&world)
            };
            // Checked rather than trusted, because the consumer resolves every identifier in here
            // with an unchecked lookup on a thread this process does not own: a dangling one is a
            // panic nothing here can catch. The projection filters relations already; this is the
            // guard that says so.
            debug_assert!(
                zgui_a11y::dangling(&update, builder.retained()).is_empty(),
                "an accessibility update named a node nothing resolves: {:?}",
                zgui_a11y::dangling(&update, builder.retained())
            );
            update
        });
        self.published_focus = self.router.interaction().focus.focused();
    }

    /// Publishes an update that says only where focus is.
    fn push_focus_only(&mut self) {
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let placements = self.host.placements();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: self.scale,
            focus: self.router.interaction().focus.focused(),
        };
        let builder = &self.a11y;
        self.surface
            .push_a11y_update(&mut || builder.focus_update(&world));
        drop(document);
        drop(layout);
        drop(placements);
        self.published_focus = self.router.interaction().focus.focused();
    }

    /// Whether focus has moved since the last update was published.
    ///
    /// Focus rides on every update, so a frame that moved focus and changed no node still owes one.
    fn focus_moved_since_publish(&self) -> bool {
        self.published_focus != self.router.interaction().focus.focused()
    }

    /// Carries out what an assistive technology asked for, if it named a node of this window.
    ///
    /// Answers whether it did. A request naming another window's node — or an action this build
    /// cannot perform — is left for somebody else rather than absorbed, because an assistive
    /// technology told an action succeeded when nothing happened tells its user the application
    /// responded.
    pub fn apply_a11y_action(&mut self, request: &ActionRequest, timestamp: Timestamp) -> bool {
        let Some(intent) = zgui_a11y::intent_of(request) else {
            return false;
        };
        let node = match &intent {
            Intent::Dispatch { node, .. }
            | Intent::Focus(node)
            | Intent::Blur(node)
            | Intent::Step { node, .. }
            | Intent::SetValue { node, .. }
            | Intent::ScrollIntoView(node)
            | Intent::ScrollTo { node, .. }
            | Intent::Scroll { node, .. } => *node,
        };
        if self.document.borrow().store().index_of(node).is_none() {
            return false;
        }

        match self.scope.as_ref().map(|scope| scope.owner().clone()) {
            Some(owner) => owner.with(|| self.carry_out_intent(intent, timestamp)),
            None => self.carry_out_intent(intent, timestamp),
        }
        // Everything above either dispatched an event or moved focus, and both owe the frame that
        // shows what they wrote.
        self.request_frame();
        true
    }

    /// Carries out one intent, with this window's scope current.
    fn carry_out_intent(&mut self, intent: Intent, timestamp: Timestamp) {
        match intent {
            Intent::Dispatch { node, event } => {
                self.synthesize(zgui_view_dom::id::to_view(node), event, timestamp);
            }
            Intent::Focus(node) => {
                self.move_focus(Some(node), zgui_input::FocusSource::Script, timestamp);
            }
            Intent::Blur(node) => {
                if self.router.interaction().focus.focused() == Some(node) {
                    self.move_focus(None, zgui_input::FocusSource::Script, timestamp);
                }
            }
            Intent::Step { node, by } => {
                let Some(value) = self.stepped_value(node, by) else {
                    return;
                };
                self.dispatch_value(node, value, timestamp);
            }
            Intent::SetValue { node, value } => self.dispatch_value(node, value, timestamp),
            Intent::ScrollIntoView(node) => {
                // The same call a component makes through `NodeRef::scroll_to`, so a node an
                // assistive technology asked for is brought into view by the scrolling system
                // rather than by a second implementation of it.
                self.carry_out_scroll(
                    zgui_view_dom::id::to_view(node),
                    zgui_view::ScrollTarget::IntoView,
                    zgui_view::ScrollBehavior::default(),
                );
            }
            Intent::ScrollTo { node, offset } => {
                // Reported in CSS pixels, because that is the space the root's transform
                // establishes and therefore the only space an assistive technology has ever been
                // shown a number in.
                let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a scroll offset is a pixel"
                )]
                let to = zgui_geom::Point::new(
                    zgui_geom::DevicePx(offset.x as f32 * scale),
                    zgui_geom::DevicePx(offset.y as f32 * scale),
                );
                self.carry_out_scroll(
                    zgui_view_dom::id::to_view(node),
                    zgui_view::ScrollTarget::Offset(to),
                    zgui_view::ScrollBehavior::default(),
                );
            }
            Intent::Scroll { node, by } => {
                self.scroll_by(node, delta_for(by), zgui_vocab::ScrollPhase::Discrete)
            }
        }
    }

    /// Where a measured control's value lands after one step, when it declared one at all.
    fn stepped_value(&self, node: zgui_dom::NodeKey, by: zgui_a11y::Step) -> Option<String> {
        let document = self.document.borrow();
        let numeric = document
            .store()
            .columns()
            .semantics
            .get(node)
            .and_then(|slot| slot.as_deref())
            .map(|semantics| semantics.numeric)?;
        zgui_a11y::action::stepped(&numeric, by).map(|value| value.to_string())
    }

    /// Dispatches a settled value change, which is the event a control's own handler already reads.
    fn dispatch_value(&mut self, node: zgui_dom::NodeKey, value: String, timestamp: Timestamp) {
        let payload = Payload::Value(ValueEvent::new(value, ValueChange::Committed));
        self.dispatch_synthetic(node, EventKind::Change, payload, timestamp);
    }
}

/// The scroll one directional action asks for, as a wheel would have reported it.
fn delta_for(by: zgui_a11y::Scroll) -> zgui_vocab::ScrollDelta {
    let lines = |x: f32, y: f32| zgui_vocab::ScrollDelta::Lines { x, y };
    match by {
        zgui_a11y::Scroll::Up => lines(0.0, -1.0),
        zgui_a11y::Scroll::Down => lines(0.0, 1.0),
        zgui_a11y::Scroll::Left => lines(-1.0, 0.0),
        zgui_a11y::Scroll::Right => lines(1.0, 0.0),
    }
}
