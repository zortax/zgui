//! Every button on the gallery keeps its label while the pointer moves over it and away.

#[path = "../examples/gallery/app.rs"]
#[allow(dead_code, reason = "the gallery names the window size it ships at")]
mod app;
mod desktop;
mod device;
#[path = "../examples/gallery/section/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "the gallery's sections are one module"
)]
mod section;
#[path = "../examples/gallery/shell.rs"]
#[allow(dead_code, reason = "the shell is one module")]
mod shell;

use std::sync::{Arc, Mutex};

use zgui::geom::{CssPx, Device, DevicePx, Point, Rect};
use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::IntoView;
use zgui::view;
use zgui::vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

use crate::app::GalleryProps;
use crate::desktop::census::Census;
use crate::desktop::grab::{self, Grab};
use crate::device::Log;

const WIDTH: f32 = 3840.0;
const HEIGHT: f32 = 2125.0;
const SCALE: f64 = 1.2;

const LABELS: [&str; 12] = [
    "Secondary",
    "Destructive",
    "Outline",
    "Ghost",
    "Link",
    "Extra small",
    "Small",
    "Medium",
    "Large",
    "New",
    "Next",
    "Default",
];

fn ink(log: &Log, rect: Rect<DevicePx, Device>) -> usize {
    let frames = log.lock().unwrap_or_else(|held| held.into_inner());
    let frame = frames.last().expect("a frame was drawn");
    let size = frame.composed.size();
    let left = (rect.origin.x.0.floor() as i32).clamp(0, size.width - 1);
    let top = (rect.origin.y.0.floor() as i32).clamp(0, size.height - 1);
    let right = ((rect.origin.x.0 + rect.size.width.0).ceil() as i32).clamp(left, size.width);
    let bottom = ((rect.origin.y.0 + rect.size.height.0).ceil() as i32).clamp(top, size.height);
    let mut counts: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();
    let mut total = 0;
    for y in top..bottom {
        for x in left..right {
            *counts.entry(frame.composed.rgba(x, y)).or_default() += 1;
            total += 1;
        }
    }
    total - counts.values().copied().max().unwrap_or(0)
}

fn pointer(at: Point<DevicePx, Device>, action: PointerAction) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(
            CssPx(at.x.0 / SCALE as f32),
            CssPx(at.y.0 / SCALE as f32),
        )),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

fn drive<'a>(
    log: &'a Log,
    failures: &'a Mutex<Vec<String>>,
) -> impl FnOnce(Box<dyn AppHandler>) -> Result<(), PlatformError> + 'a {
    move |handler| {
        let mut harness = zgui_platform_headless::Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: SCALE,
            size: zgui::geom::Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
        });
        harness.settle(128);
        let handles = grab::taken().expect("the marker view was built");
        let census = Census::take(&handles);
        let labels: Vec<(&str, Rect<DevicePx, Device>)> = LABELS
            .iter()
            .filter_map(|text| {
                census
                    .control(text)
                    .and_then(|seen| seen.rect.map(|rect| (*text, rect)))
            })
            .collect();
        assert!(
            labels.len() >= 10,
            "found only {} of the labels: {labels:?}",
            labels.len()
        );
        let at_rest: Vec<usize> = labels.iter().map(|(_, rect)| ink(log, *rect)).collect();
        let mut fails = failures.lock().unwrap_or_else(|held| held.into_inner());
        let mut entered = false;
        let frame = std::time::Duration::from_millis(8);
        let check = |harness: &mut zgui_platform_headless::Harness<_>,
                         when: String,
                         fails: &mut Vec<String>| {
            harness.settle(16);
            for (label, rect) in labels.iter() {
                let now = ink(log, *rect);
                let rest = at_rest[labels.iter().position(|(l, _)| l == label).expect("listed")];
                if now * 10 < rest * 7 {
                    fails.push(format!(
                        "{when}: {label:?} has {now} ink pixels ({rest} at rest)"
                    ));
                }
            }
        };
        for round in 0..2 {
            for (text, rect) in labels.iter() {
                let centre = Point::new(
                    DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
                    DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
                );
                if !entered {
                    harness.deliver_to_first(pointer(centre, PointerAction::Entered));
                    entered = true;
                }
                harness.deliver_to_first(pointer(centre, PointerAction::Moved));
                check(
                    &mut harness,
                    format!("round {round}: hovering {text:?}"),
                    &mut fails,
                );
                for step in 0..24 {
                    harness.advance(frame);
                    check(
                        &mut harness,
                        format!("round {round}: {step} frames after hovering {text:?}"),
                        &mut fails,
                    );
                }
            }
            // Ghost and outline, back and forth faster than the transition.
            let pair: Vec<Rect<DevicePx, Device>> = ["Outline", "Ghost"]
                .iter()
                .filter_map(|text| labels.iter().find(|(l, _)| l == text).map(|(_, r)| *r))
                .collect();
            for flip in 0..16 {
                let rect = pair[flip % pair.len()];
                let centre = Point::new(
                    DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
                    DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
                );
                harness.deliver_to_first(pointer(centre, PointerAction::Moved));
                check(
                    &mut harness,
                    format!("round {round}: flip {flip}"),
                    &mut fails,
                );
                harness.advance(frame);
                check(
                    &mut harness,
                    format!("round {round}: flip {flip}, next frame"),
                    &mut fails,
                );
            }
        }
        harness.shut_down();
        Ok(())
    }
}

#[test]
fn hovering_every_gallery_button_keeps_every_label() {
    if !device::available() {
        eprintln!("skipped: no usable graphics device");
        return;
    }
    let _guard = device::device_lock();
    device::use_real_damage();
    grab::forget();
    let log: Log = Arc::new(Mutex::new(Vec::new()));
    let failures = Mutex::new(Vec::new());
    zgui::app()
        .with_title("gallery")
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET)
        .with_renderer(Box::new(device::factory(&log)))
        .run_on(drive(&log, &failures), || {
            (Grab, view! { Gallery() }.into_view())
        })
        .expect("the gallery ran");
    let failures = failures
        .into_inner()
        .unwrap_or_else(|held| held.into_inner());
    assert!(failures.is_empty(), "{failures:#?}");
}
