//! Scrolling the focused field so that its caret stays in view.
//!
//! The reveal runs between the layout pass and the fragment pass. So the frame that moves the
//! caret also shows it, and no frame draws a caret that its field cuts off.
//!
//! It runs only when the selection or the size of the field changes. A field that a person scrolled
//! keeps that position until the next edit or caret move.
//!
//! A drag that selects text does not use this. It scrolls the field at a speed that the pointer
//! sets, in [`pointer_text`](crate::window::pointer_text).

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::scroll_region::{ScrollRegion, code_region_of_element};
use zgui_scroll::into_view::{Align, offset_for};

use crate::caret::place::Located;
use crate::window::Window;

/// The caret and field that the last reveal measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Revealed {
    /// The focused field.
    node: NodeKey,
    /// The selection that was shown.
    selection: zgui_edit::Selection,
    /// The size of the field's scrollport.
    port: Size<DevicePx, Device>,
}

/// One field's caret, measured from the layout pass.
struct CaretInPort {
    /// The region the field scrolls.
    region: ScrollRegion,
    /// The caret, in the field's content space with no scroll offset applied.
    caret: Rect<DevicePx, Device>,
}

impl Window {
    /// Scrolls the focused field so that its caret is in view, if the caret or the field changed.
    ///
    /// Call it after the layout pass and before the fragment pass, which shows the new offset.
    pub(crate) fn reveal_caret(&mut self) {
        let Some(node) = self.router.interaction().focus.focused() else {
            self.revealed = None;
            return;
        };
        let Some(selection) = self.editors.shown_selection(node) else {
            return;
        };
        let Some(measured) = self.caret_in_port(node, selection.focus, selection.affinity) else {
            return;
        };
        let key = Revealed {
            node,
            selection,
            port: measured.region.scrollport.size,
        };
        if self.revealed == Some(key) {
            return;
        }
        self.revealed = Some(key);
        // A selecting drag scrolls the field itself. A reveal here would move the text under a
        // pointer that is still inside the field.
        if self.selecting.is_some() {
            return;
        }
        let at = self.scroll.borrow().offset_of(node);
        let port = Rect::new(at, measured.region.scrollport.size);
        let mut to = offset_for(at, measured.caret, port, Align::Nearest);
        // A field with one line keeps its vertical offset: its line can be taller than the
        // scrollport, and the reveal must not move it up or down.
        if !self.is_multiline(node) {
            to.y = at.y;
        }
        if to != at {
            self.place_in_frame(node, to);
        }
    }

    /// Where the caret for one offset is in a field that code can scroll.
    ///
    /// Read from the layout pass alone, so this frame's layout gives the answer before the
    /// fragment pass runs. Nothing when the element scrolls nothing or has no laid-out text.
    fn caret_in_port(
        &self,
        node: NodeKey,
        offset: usize,
        affinity: zgui_edit::Affinity,
    ) -> Option<CaretInPort> {
        let layout = self.layout.borrow();
        let region = code_region_of_element(&layout, node)?;
        let located = Located::laid_out(&layout, &self.text, node)?;
        let caret = located.caret(offset, affinity)?;
        let container = *layout.boxes_of(node).first()?;
        let (text, _) =
            zgui_layout::scroll_region::into_view::target_in(&layout, container, located.owner)?;
        // The caret is measured from the corner of the text box's content box.
        let inset = layout.layout_of(located.owner)?;
        let origin = Point::<DevicePx, Device>::new(
            DevicePx(text.origin.x.0 + inset.border.left.0 + inset.padding.left.0),
            DevicePx(text.origin.y.0 + inset.border.top.0 + inset.padding.top.0),
        );
        Some(CaretInPort {
            region,
            caret: Rect::new(
                Point::new(
                    DevicePx(origin.x.0 + caret.origin.x.0),
                    DevicePx(origin.y.0 + caret.origin.y.0),
                ),
                Size::new(
                    DevicePx(crate::caret::caret_width(self.scale)),
                    DevicePx(caret.height.0),
                ),
            ),
        })
    }

    /// Whether an element's text was laid out on more than one line.
    pub(crate) fn is_multiline(&self, node: NodeKey) -> bool {
        let layout = self.layout.borrow();
        crate::caret::place::text_box(&layout, node)
            .and_then(|box_| layout.inline_resolution(box_))
            .is_some_and(|resolution| resolution.lines.len() > 1)
    }

    /// An element's scrollport on the device, if code can scroll the element.
    ///
    /// The padding box of the element's first fragment, as the last drawn frame placed it.
    pub(crate) fn scrollport_on_device(&self, node: NodeKey) -> Option<Rect<DevicePx, Device>> {
        let layout = self.layout.borrow();
        code_region_of_element(&layout, node)?;
        let first = *layout.boxes_of(node).first()?;
        let fragment = layout.fragment(*layout.fragments_of_box(first).first()?)?;
        Some(zgui_layout::fragment::transform::placed::onto_device(
            fragment.padding_box(),
            fragment.transform,
            &self.host.placements(),
        ))
    }
}
