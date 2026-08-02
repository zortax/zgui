//! Scroll areas, resizable panes and the carousel.
//!
//! Scrolling is read from the scroller rather than from the picture, because a scroll area that
//! moves its bar and not its content, or its content and not its bar, is two different faults and
//! a capture on its own would show only that something changed.

use zgui::geom::{Css, CssPx, Size};
use zgui::view::ScrollPosition;
use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// Drives the moving surfaces.
pub(crate) fn run(stage: &mut Stage<'_>) {
    scroll_area(stage);
    resizable(stage);
    carousel(stage);
}

/// The scroll position of the scrolling box inside `panel`, if there is one.
fn position_in(
    stage: &Stage<'_>,
    panel: zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>,
) -> Option<(zgui::view::NodeId, ScrollPosition)> {
    let census = stage.census();
    census
        .inside(panel)
        .into_iter()
        .map(|node| (node.id, stage.handles().host.scroll_position(node.id)))
        .find(|(_, position)| position.content_size.height.0 > position.scrollport.height.0 + 1.0)
}

/// The scroll area: a long list in a short box.
fn scroll_area(stage: &mut Stage<'_>) {
    let Some((_census, panel)) = find::open_panel(stage, "Scroll area") else {
        stage.report.note("ScrollArea", "the panel is not laid out");
        return;
    };
    let Some((node, before)) = position_in(stage, panel) else {
        stage.report.note("ScrollArea", "nothing inside it scrolls");
        return;
    };
    stage.report.check(
        "ScrollArea",
        "it holds more than it shows",
        before.content_size.height.0 > before.scrollport.height.0,
        &format!(
            "the content is {:.0} tall and the box shows {:.0}",
            before.content_size.height.0, before.scrollport.height.0
        ),
    );
    stage.shot("surfaces-scroll-top");

    // The wheel, which is the interaction that was reported as not working.
    stage.move_to(zgui::geom::Point::new(
        zgui::geom::DevicePx(panel.origin.x.0 + panel.size.width.0 / 2.0),
        zgui::geom::DevicePx(panel.origin.y.0 + panel.size.height.0 * 0.7),
    ));
    stage.wheel((0.0, 3.0));
    let after = stage.handles().host.scroll_position(node);
    stage.report.check(
        "ScrollArea",
        "the wheel scrolls it",
        after.offset.y.0 > before.offset.y.0 + 1.0,
        &format!(
            "three notches moved the offset from {:.1} to {:.1}",
            before.offset.y.0, after.offset.y.0
        ),
    );
    stage.shot("surfaces-scroll-wheeled");

    // A trackpad gesture is a different path through the same code: pixels rather than lines, and
    // a beginning and an end around the movement.
    stage.trackpad(Size::new(CssPx(0.0), CssPx(120.0)));
    let dragged = stage.handles().host.scroll_position(node);
    stage.report.check(
        "ScrollArea",
        "a trackpad gesture scrolls it too",
        dragged.offset.y.0 > after.offset.y.0 + 1.0,
        &format!(
            "the offset went from {:.1} to {:.1}",
            after.offset.y.0, dragged.offset.y.0
        ),
    );

    // And back up, so that the wheel is shown to work in both directions rather than only down.
    stage.wheel((0.0, -20.0));
    let back = stage.handles().host.scroll_position(node);
    stage.report.check(
        "ScrollArea",
        "the wheel scrolls back up and stops at the top",
        back.offset.y.0 < dragged.offset.y.0 && back.offset.y.0 >= -0.01,
        &format!("the offset came back to {:.1}", back.offset.y.0),
    );
    let _: Size<CssPx, Css> = Size::new(CssPx(0.0), CssPx(0.0));
}

/// The resizable panes.
fn resizable(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Resizable") else {
        stage.report.note("Resizable", "the panel is not laid out");
        return;
    };
    let inbox_width = |stage: &Stage<'_>| -> Option<f32> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| node.text == "InboxArchive")
            .filter_map(|node| node.rect)
            .map(|rect| rect.size.width.0)
            .max_by(f32::total_cmp)
    };
    let before = inbox_width(stage);
    stage.report.check(
        "Resizable",
        "both panes are laid out",
        before.is_some() && find::at_in(&census, panel, "The message, and what it says.").is_some(),
        &format!("the first pane is {before:?} device pixels wide"),
    );
    stage.shot("surfaces-resizable-before");

    // The handle is the tall thin box between the two panes, and where that is has to be asked
    // again after anything has moved it.
    let find_handle = |stage: &Stage<'_>| {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| {
                node.text.is_empty()
                    && node
                        .rect
                        .is_some_and(|rect| rect.size.height.0 > rect.size.width.0 * 4.0)
            })
            .min_by(|left, right| left.area().total_cmp(&right.area()))
            .and_then(|node| node.centre())
    };
    let Some(handle) = find_handle(stage) else {
        stage
            .report
            .note("Resizable", "no handle between the panes");
        return;
    };
    stage.drag(
        handle,
        zgui::geom::Point::new(
            zgui::geom::DevicePx(handle.x.0 + 220.0),
            zgui::geom::DevicePx(handle.y.0),
        ),
    );
    let dragged = inbox_width(stage);
    stage.report.check(
        "Resizable",
        "dragging the handle moves the split",
        before
            .zip(dragged)
            .is_some_and(|(before, after)| after > before + 40.0),
        &format!("the first pane went from {before:?} to {dragged:?}"),
    );
    stage.shot("surfaces-resizable-dragged");

    // The handle is a control, so the arrows have to move it too — and it is no longer where it
    // was, because the drag just moved it. Clicking where it used to be clicks into the pane that
    // now covers that column, and two arrows into a pane move nothing.
    let Some(moved) = find_handle(stage) else {
        stage
            .report
            .note("Resizable", "the handle is not there after the drag");
        return;
    };
    stage.click(moved);
    stage.key(NamedKey::ArrowLeft);
    stage.key(NamedKey::ArrowLeft);
    let keyed = inbox_width(stage);
    stage.report.check(
        "Resizable",
        "the arrows move the split as well",
        dragged
            .zip(keyed)
            .is_some_and(|(before, after)| (after - before).abs() > 2.0),
        &format!("two left arrows took it from {dragged:?} to {keyed:?}"),
    );
}

/// The carousel, stepped one slide at a time.
fn carousel(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Carousel") else {
        stage.report.note("Carousel", "the panel is not laid out");
        return;
    };
    let showing = |stage: &Stage<'_>| -> Vec<String> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| {
                ["One", "Two", "Three"].contains(&node.text.as_str()) && node.area() > 0.0
            })
            .map(|node| node.text.clone())
            .collect()
    };
    let before = showing(stage);
    stage.report.check(
        "Carousel",
        "its slides are laid out",
        !before.is_empty(),
        &format!("the slides with boxes are {before:?}"),
    );
    stage.shot("surfaces-carousel-first");

    // The two controls carry no text; the later one in the row is next.
    let controls: Vec<_> = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0 && node.area() < 4000.0)
        .filter_map(|node| node.rect)
        .collect();
    let Some(next) = controls
        .iter()
        .max_by(|left, right| left.origin.x.0.total_cmp(&right.origin.x.0))
    else {
        stage.report.note("Carousel", "no controls");
        return;
    };
    let offset_before = carousel_offset(stage, panel);
    stage.click(zgui::geom::Point::new(
        zgui::geom::DevicePx(next.origin.x.0 + next.size.width.0 / 2.0),
        zgui::geom::DevicePx(next.origin.y.0 + next.size.height.0 / 2.0),
    ));
    stage.settle(10);
    let offset_after = carousel_offset(stage, panel);
    stage.report.check(
        "Carousel",
        "the next control moves the slides along",
        offset_before
            .zip(offset_after)
            .is_some_and(|(before, after)| (before - after).abs() > 4.0),
        &format!("the first slide's left edge went from {offset_before:?} to {offset_after:?}"),
    );
    stage.shot("surfaces-carousel-second");
}

/// Where the carousel's first slide starts, relative to its panel.
fn carousel_offset(
    stage: &Stage<'_>,
    panel: zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>,
) -> Option<f32> {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == "One" && node.area() > 0.0)
        .filter_map(|node| node.rect)
        .map(|rect| rect.origin.x.0 - panel.origin.x.0)
        .next()
}
