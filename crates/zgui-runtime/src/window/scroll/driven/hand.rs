//! Handing scroll gestures to the compositor.

use compositor_types::{GesturePhase, ScrollGesture, ScrollRouting, ScrollSource};
use zgui_dom::NodeKey;
use zgui_vocab::{EventKind, PointerKind, ScrollPhase, WheelEvent};

use zgui_profile::{Counter, counter};

use crate::window::Window;
use crate::window::scroll::driven::Owner;

impl Window {
    /// Hands a wheel or touchpad event to the compositor before it is queued, and reports
    /// whether the compositor took it.
    ///
    /// An event whose path holds a wheel listener is left to the frame, which dispatches it first
    /// and then hands it over as the framework's default.
    pub(crate) fn hand_wheel_early(&mut self, wheel: &WheelEvent) -> bool {
        if !self.driven.on {
            return false;
        }
        if continues(wheel.phase) && self.driven.owner.is_some() {
            return self.hand_scroll(None, wheel);
        }
        let (container, listened) = self.wheel_route(wheel);
        if listened {
            // The frame decides who moves this gesture, so its later steps wait for that frame.
            self.driven.owner = None;
            return false;
        }
        self.hand_scroll(container, wheel)
    }

    /// Hands a wheel or touchpad event to the compositor when the compositor moves the content,
    /// and reports whether it did, so the window's own scrolling must leave the event alone.
    ///
    /// The step that starts a gesture decides who moves its content. The compositor does when
    /// `container`, the innermost element that scrolls under the pointer, has a scroll node. The
    /// rest of the gesture follows that decision.
    pub(crate) fn hand_scroll(&mut self, container: Option<NodeKey>, wheel: &WheelEvent) -> bool {
        if !self.driven.on {
            return false;
        }
        if !continues(wheel.phase) {
            let driven = container.is_some_and(|container| self.has_scroll_node(container));
            self.driven.owner = Some(if driven {
                Owner::Compositor
            } else {
                Owner::Window
            });
            self.driven.latched = container;
            self.driven.routed = false;
            if wheel.phase == ScrollPhase::Started {
                self.driven.track = zgui_scroll::Track::touching();
            }
        }
        if self.driven.owner != Some(Owner::Compositor) {
            return false;
        }

        let units = container
            .or(self.driven.latched)
            .or_else(|| zgui_view_dom::id::to_document(self.dom.root_node()));
        let Some(units) = units else {
            return true;
        };
        let at = self.clock.timestamp().since_origin();
        let mut delta = self.device_delta(units, wheel.delta);
        if wheel.phase != ScrollPhase::Discrete {
            delta = self.driven.track.moved(delta, at);
        }
        let phase = match wheel.phase {
            ScrollPhase::Started => GesturePhase::Begin,
            ScrollPhase::Moved => GesturePhase::Update,
            ScrollPhase::Ended => {
                let velocity = self.driven.track.velocity();
                GesturePhase::End {
                    velocity: [velocity.width.0, velocity.height.0],
                }
            }
            ScrollPhase::Momentum => GesturePhase::Momentum,
            _ => GesturePhase::Step,
        };
        let source = match (wheel.phase, wheel.kind) {
            (ScrollPhase::Discrete, _) => ScrollSource::Wheel,
            (_, PointerKind::Touch) => ScrollSource::Touch,
            _ => ScrollSource::Touchpad,
        };
        let gesture = ScrollGesture {
            position: [
                wheel.position.x.0 * self.scale,
                wheel.position.y.0 * self.scale,
            ],
            delta: [delta.width.0, delta.height.0],
            phase,
            source,
            time_ns: u64::try_from(at.as_nanos()).unwrap_or(u64::MAX),
        };
        counter::bump(Counter::GesturesHanded);
        self.driven.handed.push(gesture);
        match self.compositor.scroll_input(gesture) {
            ScrollRouting::Handled { node, .. } => {
                self.driven.routed = true;
                self.moving(node);
            }
            // A gesture that begins without moving is routed by its first movement.
            ScrollRouting::Unhandled if wheel.phase == ScrollPhase::Started => {}
            // A gesture the compositor routes to no node is the window's to scroll.
            ScrollRouting::Unhandled if !self.driven.routed => {
                self.driven.owner = Some(Owner::Window);
                return false;
            }
            ScrollRouting::Unhandled => {}
        }
        true
    }

    /// Hands the compositor again the steps it lost when the client was registered again, after
    /// the commit that gave it the nodes again.
    pub(crate) fn hand_lost_steps(&mut self) {
        if !std::mem::take(&mut self.driven.lost) {
            return;
        }
        for gesture in self.driven.handed.clone() {
            if let ScrollRouting::Handled { node, .. } = self.compositor.scroll_input(gesture) {
                self.moving(node);
            }
        }
    }

    /// Notes that the compositor built a frame, which took every step handed to it.
    pub(crate) fn steps_built(&mut self) {
        self.driven.handed.clear();
    }

    /// Notes that the compositor moves `node`, and asks for the frames that show it.
    ///
    /// A node that moves for the first time asks for a frame of the window too, which records the
    /// margin around its port before the compositor shows any of it.
    pub(super) fn moving(&mut self, node: compositor_types::ScrollId) {
        let container = self.element_of_scroll(node);
        if let Some(container) = container {
            self.scroll.borrow_mut().halt(container);
        }
        let active = self.driven.active.entry(node).or_default();
        if active.vouched.is_none() {
            self.request_frame();
        }
        self.driven.moving = true;
        self.request_compositor_frame();
    }

    /// The element whose content the scroll node `node` scrolls.
    pub(super) fn element_of_scroll(
        &mut self,
        node: compositor_types::ScrollId,
    ) -> Option<NodeKey> {
        let container = self.painter.compositor()?.scroll_container(node)?;
        self.layout.borrow().get(container)?.source
    }

    /// Reports whether the content of `container` is drawn through a scroll node.
    fn has_scroll_node(&mut self, container: NodeKey) -> bool {
        self.scroll_node_of(container).is_some()
    }

    /// Returns the scroll node the content of `container` is drawn through.
    pub(super) fn scroll_node_of(
        &mut self,
        container: NodeKey,
    ) -> Option<compositor_types::ScrollId> {
        let first = self.layout.borrow().boxes_of(container).first().copied()?;
        let tables = &self.tables;
        self.painter.compositor()?.scroll_node(tables, first)
    }

    /// The innermost element that scrolls under `wheel`, and whether its path listens to wheel
    /// events.
    fn wheel_route(&self, wheel: &WheelEvent) -> (Option<NodeKey>, bool) {
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        let filter = self.engine.filter();
        let world = zgui_input::World {
            document: &document,
            layout: &layout,
            hit: &self.hit,
            clips: &self.tables.clips,
            spatial: &self.tables.spatial,
            scale: zgui_geom::Scale::new(self.scale),
            filter: &filter,
        };
        let chain = world.chain_at(zgui_geom::Point::new(
            zgui_geom::DevicePx(wheel.position.x.0 * self.scale),
            zgui_geom::DevicePx(wheel.position.y.0 * self.scale),
        ));
        let container = match zgui_input::dispatch::defaults::on_wheel(
            document.store(),
            &layout,
            &chain,
            wheel.delta,
            wheel.phase,
        ) {
            Some(zgui_input::FrameworkDefault::Scroll { container, .. }) => Some(container),
            _ => None,
        };
        let mut plan = zgui_input::dispatch::Plan::default();
        zgui_input::dispatch::resolve(document.store(), &chain, EventKind::Wheel, &mut plan);
        (container, !plan.steps().is_empty())
    }
}

/// Reports whether `phase` continues a gesture an earlier step started.
fn continues(phase: ScrollPhase) -> bool {
    matches!(
        phase,
        ScrollPhase::Moved | ScrollPhase::Ended | ScrollPhase::Momentum
    )
}
