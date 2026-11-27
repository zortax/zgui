//! Boxes painted after a drawing cover it, under the transforms a node graph uses.
//!
//! A pan and zoom layer holds stroked edges, each under a translation of its own, and a label box
//! over the middle of each edge. The label is emitted after its edge, so every label shows its own
//! fill where the transforms put it and no edge ink, whichever raster route the edge takes.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;
use zgui::vocab::{PropValue, SharedString};

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff }
    .pane { position: relative; width: 900px; height: 600px; overflow: hidden }
    .layer { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0 }
    .edge { position: absolute; left: 0; top: 0; color: transparent;
            --zgui-stroke: #ff0000; --zgui-stroke-width: 6px }
    .label { position: absolute; left: 0; top: 0; width: 40px; height: 24px;
             background-color: #00ff00 }";

/// Edges as (origin, size), each a diagonal across its box, with a label on its midpoint.
const EDGES: [((f32, f32), (f32, f32)); 4] = [
    ((40.0, 40.0), (160.0, 120.0)),
    ((260.0, 60.0), (420.0, 300.0)),
    ((80.0, 300.0), (600.0, 200.0)),
    ((500.0, 20.0), (120.0, 90.0)),
];

fn scene(pan: RwSignal<(f32, f32)>, zoom: RwSignal<f32>) -> impl Fn() -> AnyView {
    move || {
        let edges: Vec<AnyView> = EDGES
            .iter()
            .map(|&((x, y), (w, h))| {
                let d = format!("M0 0 C{} 0 {} {h} {w} {h}", w / 2.0, w / 2.0);
                let (lx, ly) = (x + w / 2.0 - 20.0, y + h / 2.0 - 12.0);
                AnyView::new(view! {
                    vector(
                        class = "edge",
                        style:transform = move || Some(format!("translate({x}px, {y}px)")),
                        style:width = move || Some(format!("{w}px")),
                        style:height = move || Some(format!("{h}px")),
                        prop:d = {PropValue::from(SharedString::from(d))}
                    ) {}
                    box(class = "label", style:transform = move || Some(format!("translate({lx}px, {ly}px)"))) {}
                })
            })
            .collect();
        AnyView::new(view! {
            box(class = "pane") {
                box(class = "layer", style:transform = move || {
                    let (x, y) = pan.get();
                    Some(format!("translate({x}px, {y}px) scale({})", zoom.get()))
                }) {
                    {edges}
                }
            }
        })
    }
}

/// The centre part of each label on the surface, inset so edge antialiasing stays outside.
fn labels(pan: (f32, f32), zoom: f32) -> Vec<Rect<DevicePx, Device>> {
    EDGES
        .iter()
        .map(|&((x, y), (w, h))| {
            let (lx, ly) = (x + w / 2.0 - 20.0, y + h / 2.0 - 12.0);
            Rect::new(
                Point::new(
                    DevicePx(pan.0 + (lx + 3.0) * zoom),
                    DevicePx(pan.1 + (ly + 3.0) * zoom),
                ),
                Size::new(DevicePx(34.0 * zoom), DevicePx(18.0 * zoom)),
            )
        })
        .collect()
}

#[test]
fn a_label_covers_its_edge_while_the_view_pans_and_zooms() {
    crate::device::use_real_damage();
    let pan = RwSignal::new((0.0f32, 0.0f32));
    let zoom = RwSignal::new(1.0f32);
    let Some(mut stage) = Stage::open(SHEET, scene(pan, zoom)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.wait_quietly(std::time::Duration::from_millis(34));
    let steps: [((f32, f32), f32); 8] = [
        ((0.0, 0.0), 1.0),
        ((13.5, 7.25), 1.0),
        ((40.0, 20.0), 0.8),
        ((-30.0, 10.0), 1.3),
        ((120.0, 60.0), 0.6),
        ((5.3, -12.7), 1.07),
        ((-60.0, -40.0), 1.5),
        ((0.0, 0.0), 1.0),
    ];
    for (step, &(at, scale)) in steps.iter().enumerate() {
        pan.set(at);
        zoom.set(scale);
        stage.wait_quietly(std::time::Duration::from_millis(34));
        for (index, label) in labels(at, scale).into_iter().enumerate() {
            // Anything but the label's own green is edge ink over it, or a label not drawn there.
            let colours = stage.composed_colours_in(label);
            let other = colours
                .iter()
                .filter(|(r, g, b)| *r > 15 || *g < 240 || *b > 15)
                .count();
            if other > 0 {
                stage.capture_composed(&format!("labelled-{step}"));
            }
            assert_eq!(
                other,
                0,
                "step {step}, label {index}: {other} of {} pixels inside the label are not its fill",
                colours.len()
            );
        }
    }
}
