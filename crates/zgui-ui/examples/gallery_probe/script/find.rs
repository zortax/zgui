//! Aiming at things by what they say.
//!
//! Every step in the script names its target the way a person would — the button that says
//! `Outline`, the trigger inside the panel headed `Dialog` — rather than by a coordinate, so a
//! layout change moves the step with it instead of quietly moving it onto something else.

use zgui::geom::{Device, DevicePx, Point, Rect};

use crate::stage::Stage;
use crate::stage::census::Census;

/// The middle of the control that says `text`, inside `panel`.
///
/// The *smallest* node that says it, because several nested nodes carry one label and the largest
/// of them is the row the control sits at one end of. Aiming at the centre of a row is aiming at
/// the space beside an intrinsically sized button — which is not the button, answers nothing, and
/// reads exactly like a control that does not work.
pub(crate) fn at_in(
    census: &Census,
    panel: Rect<DevicePx, Device>,
    text: &str,
) -> Option<Point<DevicePx, Device>> {
    census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.centre())
}

/// The panel headed `title`, as a rectangle.
pub(crate) fn panel(census: &Census, title: &str) -> Option<Rect<DevicePx, Device>> {
    census.panel(title).and_then(|node| node.rect)
}

/// A point `inset` device pixels in from the left edge of `rect`, vertically `fraction` down it.
pub(crate) fn left_edge(
    rect: Rect<DevicePx, Device>,
    inset: f32,
    fraction: f32,
) -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(rect.origin.x.0 + inset),
        DevicePx(rect.origin.y.0 + rect.size.height.0 * fraction),
    )
}

/// Records a rectangle in the report, so that what a capture is cropped to comes from the
/// document rather than from a number somebody wrote down.
pub(crate) fn mark(stage: &mut Stage<'_>, name: &str, rect: Rect<DevicePx, Device>) {
    stage.report.rect(
        name,
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    );
}

/// Records the rectangle of the panel headed `title`, and answers with it.
pub(crate) fn mark_panel(
    stage: &mut Stage<'_>,
    census: &Census,
    title: &str,
) -> Option<Rect<DevicePx, Device>> {
    let rect = panel(census, title)?;
    mark(stage, &format!("panel:{title}"), rect);
    Some(rect)
}

/// Brings the panel headed `title` into view, and answers with a census taken once it is there.
///
/// The census has to be retaken rather than adjusted, because scrolling the page moves every box
/// in it and a step aiming at a coordinate measured before the scroll would land somewhere else
/// entirely — which looks exactly like a control that does not answer.
pub(crate) fn open_panel(
    stage: &mut Stage<'_>,
    title: &str,
) -> Option<(Census, Rect<DevicePx, Device>)> {
    let node = stage.census().panel(title)?.id;
    stage.reveal(node);
    let census = stage.census();
    let rect = panel(&census, title)?;
    mark(stage, &format!("panel:{title}"), rect);
    Some((census, rect))
}
