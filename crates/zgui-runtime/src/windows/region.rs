//! Keeping the part of a window that takes input equal to the boxes of some of its elements.

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Size};
use zgui_reactive::RenderEffect;
use zgui_reactive::prelude::*;
use zgui_view::NodeRef;

use crate::windows::use_window;

/// Keeps the input region of the current window equal to the border boxes of `nodes`.
///
/// Presses and touches on the rest of the window reach whatever is behind it. A pop-over whose
/// window covers more of the screen than its card, or a stack of notifications in a tall window,
/// gives the card or the stack here and is clickable nowhere else. The region follows the boxes
/// as they move, grow and shrink, and as the window moves to a display of another density. A node
/// that is not laid out adds nothing, so a window whose nodes are all hidden takes no input.
///
/// The region lasts as long as the calling scope. When the scope ends the whole window takes input
/// again. The desktop must support input regions; see
/// [`PlatformCapabilities::input_regions`](zgui_platform::PlatformCapabilities::input_regions).
///
/// # Panics
///
/// Panics when called outside a window.
pub fn use_input_region(nodes: impl IntoIterator<Item = NodeRef>) {
    let window = use_window();
    let boxes: Vec<_> = nodes
        .into_iter()
        .map(|node| node.observe_border_box())
        .collect();
    let scale = window.scale();
    let target = window.clone();
    let effect = RenderEffect::new(move |previous: Option<Vec<Rect<CssPx, Css>>>| {
        let scale = scale.get();
        let rects: Vec<_> = boxes
            .iter()
            .filter_map(|border| border.get())
            .map(|border| to_css(border, scale))
            .collect();
        if previous.as_ref() != Some(&rects) {
            target.set_input_region(Some(rects.clone()));
        }
        rects
    });
    zgui_reactive::on_cleanup_local(move || {
        drop(effect);
        window.set_input_region(None);
    });
}

/// `border`, measured in window device pixels, in the CSS pixels a region is given in.
fn to_css(border: Rect<DevicePx, Device>, scale: f32) -> Rect<CssPx, Css> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    Rect::new(
        Point::new(
            CssPx(border.origin.x.0 / scale),
            CssPx(border.origin.y.0 / scale),
        ),
        Size::new(
            CssPx(border.size.width.0 / scale),
            CssPx(border.size.height.0 / scale),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::to_css;
    use zgui_geom::{CssPx, DevicePx, Point, Rect, Size};

    #[test]
    fn a_device_box_is_divided_back_out_of_a_fractional_scale() {
        let border = Rect::new(
            Point::new(DevicePx(101.0), DevicePx(25.0)),
            Size::new(DevicePx(400.0), DevicePx(250.0)),
        );
        let css = to_css(border, 1.25);
        assert_eq!(css.origin.x, CssPx(80.8));
        assert_eq!(css.origin.y, CssPx(20.0));
        assert_eq!(css.size.width, CssPx(320.0));
        assert_eq!(css.size.height, CssPx(200.0));
    }

    #[test]
    fn a_scale_of_nothing_is_read_as_one() {
        let border = Rect::new(
            Point::new(DevicePx(4.0), DevicePx(4.0)),
            Size::new(DevicePx(8.0), DevicePx(8.0)),
        );
        assert_eq!(to_css(border, 0.0).size.width, CssPx(8.0));
    }
}
