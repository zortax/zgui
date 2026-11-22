//! Hover under content the compositor moves.

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_vocab::{PointerId, UiState};

use crate::window::Window;

impl Window {
    /// Asks for a frame of the window when the compositor's scrolling moved an element under the
    /// pointer or out from under it, and a `:hover` rule can match that element.
    ///
    /// Only the content of the innermost container on the hover path that the compositor moved is
    /// checked. The frame that follows moves the hover before its restyle.
    pub(crate) fn wake_for_hover(&mut self) {
        if self.driven.hover_owed || !self.router.capture().is_empty() {
            return;
        }
        let Some(at) = self.router.pointers().position_of(PointerId::MOUSE) else {
            return;
        };
        let ahead = self.ahead();
        let moved = self
            .router
            .interaction()
            .hover
            .path()
            .iter()
            .rev()
            .find_map(|key| {
                ahead
                    .iter()
                    .find(|(container, _)| container == key)
                    .copied()
            });
        let Some((container, ahead)) = moved else {
            return;
        };
        if self.hover_moves(container, at, ahead) {
            self.driven.hover_owed = true;
            self.request_frame();
        }
    }

    /// Reports whether a `:hover` rule can match an element inside `container`.
    ///
    /// The answer is kept until the window's next frame.
    fn hover_sensitive(&mut self, container: NodeKey) -> bool {
        if let Some(&sensitive) = self.driven.sensitive.get(&container) {
            return sensitive;
        }
        let sensitive = {
            let filter = self.engine.filter();
            let document = self.document.borrow();
            document
                .edit(&filter, |edit| {
                    let start = edit.document().store().index_of(container);
                    let mut stack: Vec<_> = start
                        .and_then(|index| edit.document().store().core(index).first_child())
                        .into_iter()
                        .collect();
                    while let Some(index) = stack.pop() {
                        let core = edit.document().store().core(index);
                        let (element, sibling, child) = (
                            core.kind() == zgui_dom::NodeKind::Element,
                            core.next_sibling(),
                            core.first_child(),
                        );
                        if element && edit.watches(index, UiState::HOVER) {
                            return true;
                        }
                        stack.extend(sibling);
                        stack.extend(child);
                    }
                    false
                })
                .unwrap_or(true)
        };
        self.driven.sensitive.insert(container, sensitive);
        sensitive
    }

    /// Reports whether the content of `container`, shown `ahead` of the window's offset, moved an
    /// element a `:hover` rule can match under the pointer at `at` or out from under it.
    ///
    /// Content shown under the pointer that lies outside the container in the window's own
    /// layout counts as moved if a `:hover` rule can match any element inside the container.
    fn hover_moves(
        &mut self,
        container: NodeKey,
        at: Point<DevicePx, Device>,
        ahead: Size<DevicePx, Device>,
    ) -> bool {
        let shifted = Point::new(
            DevicePx(at.x.0 + ahead.width.0),
            DevicePx(at.y.0 + ahead.height.0),
        );
        let changed: Vec<NodeKey> = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let Some(first) = layout.boxes_of(container).first().copied() else {
                return false;
            };
            let port = zgui_layout::fragment::transform::placed::window_box(
                &layout,
                first,
                &self.host.placements(),
            );
            if !port.is_some_and(|port| port.contains(shifted)) {
                drop((document, layout));
                return self.hover_sensitive(container);
            }
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
            let under = world.chain_at(shifted);
            let hovered = self.router.interaction().hover.path();
            hovered
                .iter()
                .filter(|key| !under.contains(**key))
                .chain(under.path().iter().filter(|key| !hovered.contains(key)))
                .copied()
                .collect()
        };
        let filter = self.engine.filter();
        let document = self.document.borrow();
        let store = document.store();
        changed.iter().any(|key| {
            store
                .index_of(*key)
                .is_some_and(|index| store.watches(index, &filter, UiState::HOVER))
        })
    }
}
