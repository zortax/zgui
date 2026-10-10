//! Scatter pan: one canvas of circles, dragged sideways, redrawn every tick as one solid path.
//!
//! The geometry copies a plot's scatter series: the points inside the range plus a margin, each a
//! circle of four cubic segments, all of them in one path with one solid fill. The pan moves the
//! range, so every frame draws a path nothing has drawn before.
//!
//! `mask-512` and `mask-2k` are small enough for the mask route. `markers-200` is a grid of circles
//! far enough apart for the analytic route. `10k` and `100k` are plot-sized and take the general
//! route.

use std::rc::Rc;

use zgui::canvas::zgui_color::Color;
use zgui::canvas::{Brush, ShapeBuilder};
use zgui::elements::kurbo::BezPath;
use zgui::geom::{CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view::{Anchor, BuildCx, IntoView};

use crate::scenario::vector::{Lcg, drag, opened};

/// How far a circle may sit outside the range and still be drawn, in CSS pixels.
const MARGIN: f64 = 4.0;

/// A circle's radius, in CSS pixels.
const RADIUS: f64 = 3.5;

/// The control-point distance of a quarter circle drawn as one cubic.
const KAPPA: f64 = 0.552_284_749_8;

/// How many ticks the pan runs, and how far each moves the pointer.
const TICKS: usize = 120;

/// How far one tick drags, in CSS pixels.
const STEP: f32 = 2.0;

/// The tick labels' rules and the canvas sizes.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 12px }
     .vector-root { flex-direction: column; gap: 8px }
     .plot-small { width: 240px; height: 180px }
     .plot-large { width: 960px; height: 540px }
     .ticks { flex-direction: row; gap: 12px }"
);

/// What one variant draws.
#[derive(Clone, Copy)]
struct Plot {
    /// The canvas class.
    class: &'static str,
    /// The canvas width, in CSS pixels.
    width: f32,
    /// The canvas height, in CSS pixels.
    height: f32,
    /// How many points the range shows.
    visible: usize,
    /// Whether the points stand on a grid rather than at random.
    grid: bool,
}

/// The variant's plot.
fn plot(variant: &str) -> Plot {
    let (class, width, height, visible, grid) = match variant {
        "mask-512" => ("plot-small", 240.0, 180.0, 512, false),
        "mask-2k" => ("plot-small", 240.0, 180.0, 2_048, false),
        "markers-200" => ("plot-small", 240.0, 180.0, 200, true),
        "10k" => ("plot-large", 960.0, 540.0, 10_000, false),
        "100k" => ("plot-large", 960.0, 540.0, 100_000, false),
        other => panic!("unknown scatter-pan variant `{other}`"),
    };
    Plot {
        class,
        width,
        height,
        visible,
        grid,
    }
}

/// The points over `x` in `0..2` and `y` in `0..1`.
///
/// At random, twice the visible points. On a grid, 40 columns and 10 rows, so a range of width one
/// shows about 210 points, 12 pixels apart across and 18 down on the small plot.
fn data(plot: Plot) -> Rc<Vec<(f64, f64)>> {
    if plot.grid {
        return Rc::new(
            (0..40)
                .flat_map(|column| {
                    (0..10).map(move |row| {
                        (
                            (f64::from(column) + 0.5) / 20.0,
                            (f64::from(row) + 0.5) / 10.0,
                        )
                    })
                })
                .collect(),
        );
    }
    let mut random = Lcg::new(0x5CA7_7E12);
    Rc::new(
        (0..plot.visible * 2)
            .map(|_| (random.next() * 2.0, random.next()))
            .collect(),
    )
}

/// Adds one circle to `path`: a move, four cubics and a close.
fn append_circle(path: &mut BezPath, x: f64, y: f64, r: f64) {
    let k = r * KAPPA;
    path.move_to((x + r, y));
    path.curve_to((x + r, y + k), (x + k, y + r), (x, y + r));
    path.curve_to((x - k, y + r), (x - r, y + k), (x - r, y));
    path.curve_to((x - r, y - k), (x - k, y - r), (x, y - r));
    path.curve_to((x + k, y - r), (x + r, y - k), (x + r, y));
    path.close_path();
}

/// The points inside the range starting at `low`, as one path in a box `width` by `height`.
fn scatter_path(points: &[(f64, f64)], low: f64, width: f64, height: f64) -> BezPath {
    let mut path = BezPath::new();
    for &(x, y) in points {
        let px = (x - low) * width;
        if px < -MARGIN || px > width + MARGIN {
            continue;
        }
        append_circle(&mut path, px, (1.0 - y) * height, RADIUS);
    }
    path
}

/// The document: the canvas and sixteen tick labels.
fn view(plot: Plot) -> impl IntoView {
    let points = data(plot);
    let low = RwSignal::new_local(1.0_f64);
    let grab: RwSignal<Option<f32>, zgui::reactive::LocalStorage> = RwSignal::new_local(None);
    let canvas = zgui::elements::canvas()
        .class(plot.class)
        .on(
            events::PointerDown,
            move |cx: &mut EventCx<'_, events::PointerDown>| {
                grab.set(Some(cx.position.x.0));
                cx.capture_pointer();
            },
        )
        .on(
            events::PointerMove,
            move |cx: &mut EventCx<'_, events::PointerMove>| {
                if let Some(last) = grab.get_untracked() {
                    let moved = f64::from(cx.position.x.0 - last);
                    low.update(|low| *low -= moved / f64::from(plot.width));
                    grab.set(Some(cx.position.x.0));
                }
            },
        )
        .on(
            events::PointerUp,
            move |cx: &mut EventCx<'_, events::PointerUp>| {
                grab.set(None);
                cx.release_pointer();
            },
        )
        .draw(move |cx| {
            let width = f64::from(cx.size.width.0);
            let height = f64::from(cx.size.height.0);
            if width <= 0.0 {
                return;
            }
            let path = scatter_path(&points, low.get(), width, height);
            cx.scene.push(
                ShapeBuilder::new(path)
                    .fill(Brush::Solid(Color::srgb(0.36, 0.62, 1.0, 1.0)))
                    .build(),
            );
        });
    view! {
        column(class = "vector-root") {
            {canvas.into_view()}
            row(class = "ticks") {
                for index in move || 0..16_usize, key = |index: &usize| *index {
                    text {{move || format!("{:.2}", index as f64 / 16.0)}}
                }
            }
        }
    }
}

/// Runs one variant.
pub(super) fn run(variant: &str) {
    let plot = plot(variant);
    let runtime = crate::scenario::fixture::custom(SHEET, move |cx: &mut BuildCx<'_>| {
        Box::new(view(plot).into_view().build(cx)) as Box<dyn Anchor>
    });
    let mut harness = opened(runtime);
    let centre = Point::new(CssPx(plot.width / 2.0), CssPx(plot.height / 2.0));
    drag(
        &mut harness,
        ("scatter-pan", variant, "pan"),
        centre,
        STEP,
        TICKS,
    );
}
