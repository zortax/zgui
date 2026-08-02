//! The drawings the window has to go on showing, and the pictures that say whether it does.
//!
//! Each of these is a thing with ink in it — outlines from an icon set, two SVG documents, a run
//! of turned type, the chevron in a select's trigger — over a background that has none. Whether it
//! is still there is answered by counting, in the capture, the pixels that differ from the most
//! common colour inside the rectangle, and comparing that count with the same count taken before
//! the window was driven at all. A count is used rather than an exact comparison because the
//! window is deliberately being resized and reschemed between the sweeps: the ink moves and
//! changes colour, and the claim is that there is still ink.

use zgui::geom::{Device, DevicePx, Rect};

use crate::script::find;
use crate::stage::Stage;

/// The currencies a select's trigger can be showing, since earlier parts of the run change it.
const CURRENCIES: [&str; 4] = ["Pound sterling", "Euro", "US dollar", "Choose one"];

/// How wide the strip at the end of a select's trigger is, in device pixels, so that the chevron
/// is in it and the words are not.
const CHEVRON: f32 = 34.0;

/// Which part of a panel a target is.
#[derive(Copy, Clone)]
pub(crate) enum Part {
    /// The whole panel.
    Whole,
    /// One row of it, named by the word written beside it.
    Row(&'static str),
    /// The end of the select's trigger, where the chevron is.
    Chevron,
}

/// A drawing the window must go on showing, and where to find it.
#[derive(Copy, Clone)]
pub(crate) struct Target {
    /// What it is called in the report and in the file names.
    pub(crate) name: &'static str,
    /// The panel it lives in.
    pub(crate) panel: &'static str,
    /// Which part of that panel is looked at.
    pub(crate) part: Part,
}

/// Everything a sweep looks at.
pub(crate) const TARGETS: [Target; 6] = [
    Target {
        name: "icons",
        panel: "Icon",
        part: Part::Row("marks"),
    },
    Target {
        name: "scenes",
        panel: "Not tinted by its context",
        part: Part::Row("scenes"),
    },
    Target {
        name: "ramp",
        panel: "A ramp and a clip",
        part: Part::Whole,
    },
    Target {
        name: "turned",
        panel: "Turned type",
        part: Part::Whole,
    },
    Target {
        name: "chevron",
        panel: "Select and combobox",
        part: Part::Chevron,
    },
    Target {
        name: "buttons",
        panel: "Button",
        part: Part::Whole,
    },
];

/// Captures the window under `name`, having first written down the rectangle the picture is to be
/// judged over.
///
/// The order matters: a rectangle recorded after the fact could have been chosen to suit the
/// picture. This one comes out of the laid-out document immediately before the shutter.
pub(crate) fn shot_of(stage: &mut Stage<'_>, name: &str, rect: Rect<DevicePx, Device>) {
    let rect = clipped(rect, stage.window());
    stage.report.rect(
        &format!("crop:{name}"),
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    );
    stage.shot(name);
}

/// `rect` with everything outside `window` taken off it.
fn clipped(rect: Rect<DevicePx, Device>, window: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    let left = rect.origin.x.0.max(0.0);
    let top = rect.origin.y.0.max(0.0);
    let right = (rect.origin.x.0 + rect.size.width.0).min(window.size.width.0);
    let bottom = (rect.origin.y.0 + rect.size.height.0).min(window.size.height.0);
    Rect::new(
        zgui::geom::Point::new(DevicePx(left), DevicePx(top)),
        zgui::geom::Size::new(
            DevicePx((right - left).max(0.0)),
            DevicePx((bottom - top).max(0.0)),
        ),
    )
}

/// Brings `target` into view and answers with the rectangle its ink is in.
fn crop(stage: &mut Stage<'_>, target: Target) -> Option<Rect<DevicePx, Device>> {
    let (census, panel) = find::open_panel(stage, target.panel)?;
    match target.part {
        Part::Whole => Some(panel),
        // The largest box whose text *begins* with the word beside the row, which is the row
        // itself: the smallest is that word's own text node, and an exact match finds nothing
        // else whenever the things in the row say anything at all.
        Part::Row(label) => census
            .inside(panel)
            .into_iter()
            .filter(|node| node.text.starts_with(label) && node.area() > 0.0)
            .max_by(|left, right| left.area().total_cmp(&right.area()))
            .and_then(|node| node.rect),
        Part::Chevron => census
            .inside(panel)
            .into_iter()
            .filter(|node| CURRENCIES.contains(&node.text.as_str()) && node.area() > 0.0)
            .max_by(|left, right| left.area().total_cmp(&right.area()))
            .and_then(|node| node.rect)
            .map(|trigger| {
                Rect::new(
                    zgui::geom::Point::new(
                        DevicePx(trigger.origin.x.0 + trigger.size.width.0 - CHEVRON),
                        DevicePx(trigger.origin.y.0),
                    ),
                    zgui::geom::Size::new(DevicePx(CHEVRON), trigger.size.height),
                )
            }),
    }
}

/// Looks at every target in turn and captures it, under names beginning `gxp-{when}`.
///
/// The pointer is taken off the window first. A capture with the pointer resting on a control is
/// a capture of that control lit, and a sweep half of whose pictures were taken hovered would
/// differ from the one before it for a reason that has nothing to do with what is being asked.
pub(crate) fn sweep(stage: &mut Stage<'_>, when: &str) {
    stage.leave();
    for target in TARGETS {
        let Some(rect) = crop(stage, target) else {
            stage.report.note(
                "Painted",
                &format!("{}: nothing to look at in {}", target.name, target.panel),
            );
            continue;
        };
        shot_of(stage, &format!("gxp-{when}-{}", target.name), rect);
    }
}
