//! Map pan: one large canvas of polygons, lines and circles, dragged, pinched and left to settle.
//!
//! The geometry is built once. Every draw pushes every shape again, moved by the pan offset, so
//! the application hands over new geometry each frame the way a map view does. The pinch scales
//! the canvas through an inline `transform`.

use std::rc::Rc;

use zgui::canvas::zgui_color::Color;
use zgui::canvas::{Brush, ShapeBuilder};
use zgui::elements::kurbo::{Affine, BezPath};
use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui::view::{Anchor, BuildCx, IntoView};

use crate::scenario::vector::{Lcg, Stretch, drag, opened, settle};

/// The canvas size, in CSS pixels.
const WIDTH: f64 = 960.0;

/// The canvas height, in CSS pixels.
const HEIGHT: f64 = 600.0;

/// How many filled polygons the map holds.
const POLYGONS: usize = 2_400;

/// How many stroked polylines it holds.
const LINES: usize = 400;

/// How many circles it holds.
const CIRCLES: usize = 50;

/// How many ticks the pan runs.
const PAN_TICKS: usize = 120;

/// How many ticks the pinch and the settle each run.
const PINCH_TICKS: usize = 30;

/// The eight fill colours.
const PALETTE: [(f32, f32, f32); 8] = [
    (0.36, 0.62, 1.0),
    (0.94, 0.28, 0.44),
    (0.02, 0.84, 0.63),
    (1.0, 0.82, 0.4),
    (0.51, 0.22, 0.93),
    (0.07, 0.54, 0.7),
    (0.96, 0.64, 0.38),
    (0.55, 0.6, 0.68),
];

/// The canvas.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 12px }
     .vector-root { flex-direction: column }
     .map { width: 960px; height: 600px }"
);

/// One shape of the map, at the origin of the pan.
struct Feature {
    /// Its outline.
    path: BezPath,
    /// Its fill colour, or `None` for a line.
    fill: Option<usize>,
    /// Its stroke width, for a line.
    width: f64,
}

/// The map: polygons, polylines and circles, all from one seed.
fn features() -> Rc<Vec<Feature>> {
    let mut random = Lcg::new(0x0A11_CE55);
    let mut features = Vec::with_capacity(POLYGONS + LINES + CIRCLES);
    for index in 0..POLYGONS {
        let cx = random.next() * WIDTH;
        let cy = random.next() * HEIGHT;
        let radius = 4.0 + random.next() * 14.0;
        let corners = 6 + (random.next() * 19.0) as usize;
        let mut path = BezPath::new();
        for corner in 0..corners {
            let angle = std::f64::consts::TAU * corner as f64 / corners as f64;
            let reach = radius * (0.6 + 0.4 * random.next());
            let point = (cx + reach * angle.cos(), cy + reach * angle.sin());
            if corner == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close_path();
        features.push(Feature {
            path,
            fill: Some(index % PALETTE.len()),
            width: 0.0,
        });
    }
    for _ in 0..LINES {
        let mut path = BezPath::new();
        let mut x = random.next() * WIDTH;
        let mut y = random.next() * HEIGHT;
        path.move_to((x, y));
        for _ in 0..8 {
            x += (random.next() - 0.5) * 80.0;
            y += (random.next() - 0.5) * 80.0;
            path.line_to((x, y));
        }
        features.push(Feature {
            path,
            fill: None,
            width: 1.0 + (random.next() * 2.0).round(),
        });
    }
    for index in 0..CIRCLES {
        let circle = zgui::elements::kurbo::Circle::new(
            (random.next() * WIDTH, random.next() * HEIGHT),
            6.0 + random.next() * 10.0,
        );
        features.push(Feature {
            path: zgui::elements::kurbo::Shape::to_path(&circle, 0.1),
            fill: Some(index % PALETTE.len()),
            width: 0.0,
        });
    }
    Rc::new(features)
}

/// The document: the canvas, panned by a drag and scaled by `zoom`.
fn view(zoom: RwSignal<f64, LocalStorage>) -> impl IntoView {
    let features = features();
    let offset = RwSignal::new_local(0.0_f64);
    let grab: RwSignal<Option<f32>, LocalStorage> = RwSignal::new_local(None);
    let canvas = zgui::elements::canvas()
        .class("map")
        .style_property("transform", move || {
            let zoom = zoom.get();
            (zoom != 1.0).then(|| format!("scale({zoom})"))
        })
        .on(events::PointerDown, move |cx: &mut EventCx<'_, events::PointerDown>| {
            grab.set(Some(cx.position.x.0));
            cx.capture_pointer();
        })
        .on(events::PointerMove, move |cx: &mut EventCx<'_, events::PointerMove>| {
            if let Some(last) = grab.get_untracked() {
                let moved = f64::from(cx.position.x.0 - last);
                offset.update(|offset| *offset += moved);
                grab.set(Some(cx.position.x.0));
            }
        })
        .on(events::PointerUp, move |cx: &mut EventCx<'_, events::PointerUp>| {
            grab.set(None);
            cx.release_pointer();
        })
        .draw(move |cx| {
            let moved = Affine::translate((offset.get(), 0.0));
            for feature in features.iter() {
                let path = moved * feature.path.clone();
                let shape = match feature.fill {
                    Some(colour) => {
                        let (r, g, b) = PALETTE[colour];
                        ShapeBuilder::new(path).fill(Brush::Solid(Color::srgb(r, g, b, 0.9)))
                    }
                    None => ShapeBuilder::new(path).stroke(
                        Brush::Solid(Color::srgb(0.9, 0.92, 0.96, 1.0)),
                        feature.width,
                    ),
                };
                cx.scene.push(shape.build());
            }
        });
    view! {
        column(class = "vector-root") {
            {canvas.into_view()}
        }
    }
}

/// Runs one variant.
pub(super) fn run(variant: &str) {
    assert_eq!(variant, "map", "unknown map-pan variant `{variant}`");
    let zoom = RwSignal::new_local(1.0_f64);
    let runtime = crate::scenario::fixture::custom(SHEET, move |cx: &mut BuildCx<'_>| {
        Box::new(view(zoom).into_view().build(cx)) as Box<dyn Anchor>
    });
    let mut harness = opened(runtime);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the canvas size is a small integer number of CSS pixels"
    )]
    let centre = Point::new(CssPx((WIDTH / 2.0) as f32), CssPx((HEIGHT / 2.0) as f32));
    drag(&mut harness, ("map-pan", variant, "pan"), centre, 2.0, PAN_TICKS);

    let mut pinch = Stretch::begin("map-pan", variant, "pinch");
    for tick in 1..=PINCH_TICKS {
        zoom.set(1.0 + 0.6 * tick as f64 / PINCH_TICKS as f64);
        pinch.tick(&mut harness);
    }
    pinch.end();

    let mut still = Stretch::begin("map-pan", variant, "settle");
    for _ in 0..PINCH_TICKS {
        still.tick(&mut harness);
    }
    still.end();
    settle(&mut harness);
}
