//! The page's own scroll handle: that there is one, that dragging it moves the page, and that a press
//! on the groove below it moves the page by a screenful.
//!
//! A scrollbar is not part of the document. It is drawn outside the scrollport it belongs to, from the
//! scroll region rather than from any box, so there is no node to ask where it is — what says where it
//! is is the width the content lost, which is the gutter the bar is in. Everything here is aimed at
//! that gutter, and what answers is the scroller's own offset.

use zgui::geom::{DevicePx, Point};
use zgui::view::NodeId;

use crate::script::gauntlet::ink::shot_of;
use crate::stage::Stage;

/// How far down the gutter the drag starts, in device pixels.
///
/// Inside the thumb whenever the thumb starts at the top, which it does at the top of the page, and
/// far enough in that the drag is not being asked to start on the bar's own end.
const GRAB: f32 = 40.0;

/// How far the drag goes, in device pixels.
const DRAG: f32 = 300.0;

/// How far up from the bottom of the window a press on the groove lands.
const BELOW: f32 = 30.0;

/// Drives the handle.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let root = stage.handles().root();
    let window = stage.window();
    to_the_top(stage, root);

    let position = stage.handles().host.scroll_position(root);
    let gutter = window.size.width.0 - position.scrollport.width.0;
    stage.report.check(
        "Scrollbar",
        "the page keeps a gutter for its bar",
        gutter > 1.0,
        &format!(
            "the window is {:.0} wide and the scrollport {:.0}, so the gutter is {gutter:.1}",
            window.size.width.0, position.scrollport.width.0
        ),
    );
    if gutter <= 1.0 {
        return;
    }
    let bar = zgui::geom::Rect::new(
        Point::new(DevicePx(position.scrollport.width.0), DevicePx(0.0)),
        zgui::geom::Size::new(DevicePx(gutter), window.size.height),
    );
    shot_of(stage, "vd-handle-top", bar);

    let along = DevicePx(position.scrollport.width.0 + gutter / 2.0);
    let before = offset(stage, root);
    stage.drag(
        Point::new(along, DevicePx(GRAB)),
        Point::new(along, DevicePx(GRAB + DRAG)),
    );
    let dragged = offset(stage, root);
    // What the drag is worth: the same fraction of the content that the travel is of the track.
    let expected = DRAG / window.size.height.0 * position.content_size.height.0;
    stage.report.check(
        "Scrollbar",
        "dragging the handle scrolls the page",
        dragged > before + DRAG / 2.0,
        &format!(
            "a {DRAG:.0} pixel drag down the gutter took the page from {before:.0} to \
             {dragged:.0}, where the track's own share of the content is {expected:.0}"
        ),
    );
    shot_of(stage, "vd-handle-dragged", bar);

    to_the_top(stage, root);
    let before = offset(stage, root);
    stage.click(Point::new(along, DevicePx(window.size.height.0 - BELOW)));
    let paged = offset(stage, root);
    let screenful = position.scrollport.height.0;
    stage.report.check(
        "Scrollbar",
        "a press on the groove below the handle moves the page by a screenful",
        (paged - before - screenful).abs() <= screenful * 0.1,
        &format!(
            "the press took the page from {before:.0} to {paged:.0}, and a screenful is \
             {screenful:.0}"
        ),
    );
    shot_of(stage, "vd-handle-paged", bar);
    to_the_top(stage, root);
}

/// Where the page is scrolled to.
fn offset(stage: &Stage<'_>, root: NodeId) -> f32 {
    stage.handles().host.scroll_position(root).offset.y.0
}

/// Puts the page back at the top, so that the next thing measured starts where the last one did.
fn to_the_top(stage: &mut Stage<'_>, root: NodeId) {
    stage.move_to(Point::new(DevicePx(300.0), DevicePx(300.0)));
    stage.wheel((0.0, -60.0));
    stage.settle(30);
    if offset(stage, root) > 1.0 {
        stage.wheel((0.0, -60.0));
        stage.settle(30);
    }
}
