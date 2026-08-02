//! The page's own scrolling: how far a notch moves it, which way, and what survives a restyle.
//!
//! Scrolling is read from the scroller rather than from the picture. A page that scrolls and then
//! silently returns to the top looks, in a capture, exactly like a page that never scrolled, and
//! the difference between the two is the whole of what this part is for.

use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::view::NodeId;

use crate::stage::Stage;

/// Drives the page's scrolling.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some(root) = scrolling_root(stage) else {
        stage
            .report
            .note("PageScroll", "nothing in the page scrolls");
        return;
    };
    direction_and_distance(stage, root);
    survives_a_hover(stage, root);
}

/// The outermost node that has more content than it shows, which is the page itself.
fn scrolling_root(stage: &Stage<'_>) -> Option<NodeId> {
    let root = stage.handles().root();
    let position = stage.handles().host.scroll_position(root);
    (position.content_size.height.0 > position.scrollport.height.0 + 1.0).then_some(root)
}

/// Where the page is scrolled to, in device pixels.
fn offset_of(stage: &Stage<'_>, node: NodeId) -> f32 {
    stage.handles().host.scroll_position(node).offset.y.0
}

/// One notch down, then the same back up, with the distance each covered.
fn direction_and_distance(stage: &mut Stage<'_>, root: NodeId) {
    stage.move_to(Point::new(DevicePx(300.0), DevicePx(300.0)));
    let top = offset_of(stage, root);
    stage.wheel((0.0, 1.0));
    stage.settle(30);
    let one = offset_of(stage, root);
    stage.report.check(
        "PageScroll",
        "a notch away from the user moves the page down",
        one > top + 1.0,
        &format!("one notch took the page from {top:.1} to {one:.1} device pixels"),
    );

    stage.wheel((0.0, 3.0));
    stage.settle(30);
    let four = offset_of(stage, root);
    stage.report.note(
        "PageScroll",
        &format!(
            "one notch covered {:.1} device pixels and three covered {:.1}",
            one - top,
            four - one
        ),
    );

    stage.wheel((0.0, -6.0));
    stage.settle(30);
    let back = offset_of(stage, root);
    stage.report.check(
        "PageScroll",
        "a notch towards the user moves it back and stops at the top",
        back < four && back >= -0.01,
        &format!("six notches up came back to {back:.1}"),
    );

    // A trackpad reports pixels, and one gesture of 120 pixels must move the page by 120 device
    // pixels at scale one — no line height, no multiplier.
    stage.trackpad(Size::<CssPx, Css>::new(CssPx(0.0), CssPx(120.0)));
    stage.settle(20);
    let swiped = offset_of(stage, root);
    stage.report.note(
        "PageScroll",
        &format!(
            "a 120 CSS-pixel gesture moved it {:.1} device pixels at scale {:.2}",
            swiped - back,
            stage.scale()
        ),
    );
    stage.wheel((0.0, -40.0));
    stage.settle(40);
}

/// The page scrolled, then a pointer moved onto something that restyles on hover.
fn survives_a_hover(stage: &mut Stage<'_>, root: NodeId) {
    stage.move_to(Point::new(DevicePx(300.0), DevicePx(300.0)));
    stage.wheel((0.0, 6.0));
    stage.settle(40);
    let scrolled = offset_of(stage, root);
    if scrolled <= 1.0 {
        stage
            .report
            .note("PageScroll", "the page did not scroll at all");
        return;
    }

    // Something that certainly restyles when a pointer arrives: every button in the library has a
    // hover rule. Whichever one is under this point after the scroll will do.
    let census = stage.census();
    let target = census
        .nodes
        .iter()
        .filter(|node| node.area() > 200.0 && node.area() < 20_000.0)
        .filter(|node| !node.text.is_empty())
        .filter_map(|node| node.centre())
        .find(|at| at.y.0 > 60.0 && at.y.0 < 600.0 && at.x.0 > 40.0);
    let Some(at) = target else {
        stage
            .report
            .note("PageScroll", "nothing on screen to hover after the scroll");
        return;
    };
    stage.move_to(at);
    stage.settle(20);
    let hovered = offset_of(stage, root);
    stage.report.check(
        "PageScroll",
        "hovering does not move the page",
        (hovered - scrolled).abs() < 0.5,
        &format!(
            "the page was at {scrolled:.1} and a hover at ({:.0}, {:.0}) left it at {hovered:.1}",
            at.x.0, at.y.0
        ),
    );
    stage.shot("scrolling-after-hover");

    // And a press, which restyles again and is the other half of what a person does with a mouse.
    stage.press_release(
        zgui::vocab::PointerButton::Primary,
        zgui::vocab::Modifiers::NONE,
    );
    stage.settle(20);
    let pressed = offset_of(stage, root);
    stage.report.check(
        "PageScroll",
        "pressing does not move the page",
        (pressed - scrolled).abs() < 0.5,
        &format!("the page was at {scrolled:.1} and is at {pressed:.1} after a click"),
    );

    stage.wheel((0.0, -40.0));
    stage.settle(40);
}
