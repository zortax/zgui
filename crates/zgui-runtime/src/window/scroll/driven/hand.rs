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
    /// `container`, the innermost element that scrolls under the pointer, has a scroll node, the
    /// compositor's frontmost item there lies inside it, and the compositor routes the step to a
    /// node. The rest of the gesture follows that decision.
    pub(crate) fn hand_scroll(&mut self, container: Option<NodeKey>, wheel: &WheelEvent) -> bool {
        if !self.driven.on {
            return false;
        }
        if !continues(wheel.phase) {
            let driven = container.is_some_and(|container| {
                self.has_scroll_node(container) && self.shows_inside(container, wheel)
            });
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
        let routed = self.compositor.scroll_input(gesture);
        match routed {
            // compositor: a gesture over a nested port can be routed to an outer node although the
            // nested one could move. The window scrolls the nested one instead.
            ScrollRouting::Handled { node, .. }
                if !self.driven.routed && self.routed_around(node, container, delta) =>
            {
                self.moving(node);
                self.driven.owner = Some(Owner::Window);
                return false;
            }
            ScrollRouting::Handled { node, .. } => {
                self.driven.routed = true;
                self.moving(node);
            }
            // A gesture that begins without moving is routed by its first movement.
            ScrollRouting::Unhandled if wheel.phase == ScrollPhase::Started => {}
            // compositor: routing starts from an item under the point, so a gesture over a part
            // of a port that draws nothing reaches no node. The window scrolls it instead.
            ScrollRouting::Unhandled if !self.driven.routed => {
                self.driven.owner = Some(Owner::Window);
                return false;
            }
            ScrollRouting::Unhandled => {}
        }
        true
    }

    /// Notes that the compositor moves `node`, and asks for the frames that show it.
    ///
    /// A node that moves for the first time asks for a frame of the window too, which records the
    /// margin around its port before the compositor shows any of it.
    fn moving(&mut self, node: compositor_types::ScrollId) {
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
        let Some(first) = self.layout.borrow().boxes_of(container).first().copied() else {
            return false;
        };
        let tables = &self.tables;
        self.painter
            .compositor()
            .is_some_and(|sync| sync.scroll_node(tables, first).is_some())
    }

    /// Reports whether the compositor routed a step of `delta` to `node` past `container`, the
    /// innermost element that scrolls under the pointer, although `container` could move by it.
    fn routed_around(
        &mut self,
        node: compositor_types::ScrollId,
        container: Option<NodeKey>,
        delta: zgui_geom::Size<zgui_geom::DevicePx, zgui_geom::Device>,
    ) -> bool {
        let Some(container) = container else {
            return false;
        };
        if self.element_of_scroll(node) == Some(container) {
            return false;
        }
        let layout = self.layout.borrow();
        let Some(region) = zgui_layout::scroll_region::region_of_element(&layout, container) else {
            return false;
        };
        let limit = region.hand_limit();
        let at = self.scroll.borrow().offset_of(container);
        let moves =
            |at: f32, limit: f32, by: f32| (by > 0.0 && at < limit) || (by < 0.0 && at > 0.0);
        moves(at.x.0, limit.x.0, delta.width.0) || moves(at.y.0, limit.y.0, delta.height.0)
    }

    /// Reports whether the compositor's frontmost item under `wheel` is drawn by `container` or by
    /// an element inside it.
    // compositor: routing starts from the frontmost item, so a gesture over a part of a port that
    // draws nothing reaches the node of whatever is drawn behind the port, or none.
    fn shows_inside(&self, container: NodeKey, wheel: &WheelEvent) -> bool {
        let at = [
            wheel.position.x.0 * self.scale,
            wheel.position.y.0 * self.scale,
        ];
        let Some(hit) = self.compositor.hit_test(at) else {
            return false;
        };
        let Some(owner) = zgui_dom::side::BoxKey::from_u64(hit.chunk.owner) else {
            return false;
        };
        let Some(element) = self.layout.borrow().get(owner).and_then(|node| node.source) else {
            return false;
        };
        let document = self.document.borrow();
        zgui_input::HitChain::to_root(document.store(), element).contains(container)
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
