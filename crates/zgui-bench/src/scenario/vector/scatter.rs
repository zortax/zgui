//! Scatter pan: one canvas of circles, dragged sideways, redrawn every tick as one solid path.
//!
//! The geometry copies a plot's scatter series: the points inside the range plus a margin, each a
//! circle of four cubic segments, all of them in one path with one solid fill. The pan moves the
//! range, so every frame draws a path nothing has drawn before.
//!
//! `mask-512` and `mask-2k` are small enough for the mask route. `markers-200` is a grid of circles
//! far enough apart for the analytic route. `10k` and `100k` are plot-sized and take the marks
//! route. `waves` copies a plot's waves example: a grid, a damped sine as joined line segments, and
//! noisy cosine samples as circles, all with solid paints.
//!
//! `series-100k` and `series-1m` draw the scatter once as a canvas series and pan it through the
//! canvas view. `waves-view` draws the waves over the whole data range once and pans it the same
//! way. All three sit in a clipping box of the plot's size.

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
     .plot-clip { overflow: hidden; width: 960px; height: 540px }
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
    /// Whether it draws the waves plot rather than a scatter.
    waves: bool,
    /// Whether it draws once and pans through the canvas view.
    view: bool,
}

/// The variant's plot.
fn plot(variant: &str) -> Plot {
    let (class, width, height, visible, grid) = match variant {
        "mask-512" => ("plot-small", 240.0, 180.0, 512, false),
        "mask-2k" => ("plot-small", 240.0, 180.0, 2_048, false),
        "markers-200" => ("plot-small", 240.0, 180.0, 200, true),
        "10k" => ("plot-large", 960.0, 540.0, 10_000, false),
        "100k" => ("plot-large", 960.0, 540.0, 100_000, false),
        "waves" | "waves-view" => ("plot-large", 960.0, 540.0, 0, false),
        "series-100k" => ("plot-large", 960.0, 540.0, 100_000, false),
        "series-1m" => ("plot-large", 960.0, 540.0, 1_000_000, false),
        other => panic!("unknown scatter-pan variant `{other}`"),
    };
    Plot {
        class,
        width,
        height,
        visible,
        grid,
        waves: variant.starts_with("waves"),
        view: variant.starts_with("series") || variant == "waves-view",
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

/// The waves plot over the range starting at `low`, in a box `width` by `height`.
///
/// The data runs over `x` in `0..4π`; the range is one period of the data wide and the pan moves
/// it. Unless `whole`, segments and samples outside the range plus the margin are left out, as the
/// plot clips them.
fn waves(low: f64, width: f64, height: f64, whole: bool) -> Vec<zgui::canvas::Shape> {
    let span = 4.0 * std::f64::consts::PI;
    let x_of = |x: f64| (x / span - (low - 1.0)) * width;
    let y_of = |y: f64| (1.4 - y) / 2.8 * height;
    let visible = |px: f64| whole || (-MARGIN..=width + MARGIN).contains(&px);
    let mut shapes = Vec::new();
    // The grid: vertical lines at data values, horizontal lines at fixed heights, translucent.
    let grid = Color::srgb(140.0 / 255.0, 150.0 / 255.0, 170.0 / 255.0, 0.28);
    let mut vertical = BezPath::new();
    for index in -12..=24 {
        let px = x_of(f64::from(index) * span / 12.0);
        if visible(px) {
            vertical.move_to((px, 0.0));
            vertical.line_to((px, height));
        }
    }
    let mut horizontal = BezPath::new();
    for index in 0..=7 {
        let py = f64::from(index) * height / 7.0;
        horizontal.move_to((0.0, py));
        horizontal.line_to((width, py));
    }
    for path in [vertical, horizontal] {
        if !path.is_empty() {
            shapes.push(
                ShapeBuilder::new(path)
                    .stroke(Brush::Solid(grid), 1.0)
                    .build(),
            );
        }
    }
    // The wave: 601 samples, one segment per pair, joined into one path.
    let wave: Vec<(f64, f64)> = (0..=600)
        .map(|i| {
            let x = f64::from(i) * span / 600.0;
            (x, x.sin() * (-x / 8.0).exp())
        })
        .collect();
    let mut line = BezPath::new();
    for pair in wave.windows(2) {
        let (start, end) = (x_of(pair[0].0), x_of(pair[1].0));
        if visible(start) || visible(end) {
            line.move_to((start, y_of(pair[0].1)));
            line.line_to((end, y_of(pair[1].1)));
        }
    }
    if !line.is_empty() {
        shapes.push(
            ShapeBuilder::new(line)
                .stroke(Brush::Solid(Color::srgb(0.43, 0.66, 1.0, 1.0)), 2.0)
                .build(),
        );
    }
    // The samples: 80 circles of radius 3.5.
    let mut samples = BezPath::new();
    for i in 0..80 {
        let x = f64::from(i) * span / 79.0;
        let jitter = (f64::from(i) * 12.9898).sin() * 43_758.545;
        let y = x.cos() * 0.8 + (jitter - jitter.floor() - 0.5) * 0.3;
        let px = x_of(x);
        if visible(px) {
            append_circle(&mut samples, px, y_of(y), RADIUS);
        }
    }
    if !samples.is_empty() {
        shapes.push(
            ShapeBuilder::new(samples)
                .fill(Brush::Solid(Color::srgb(1.0, 0.71, 0.28, 1.0)))
                .build(),
        );
    }
    shapes
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
            if plot.waves {
                for shape in waves(low.get(), width, height, false) {
                    cx.scene.push(shape);
                }
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

/// The document for a variant that draws once and pans through the canvas view.
///
/// The scatter is one points series over twice the visible points, in data space; the waves are
/// their shapes over the whole data range. The pan sets the view to `translate((1 − low) · width)`.
fn viewed(plot: Plot) -> impl IntoView {
    let handle = CanvasHandle::new();
    let (width, height) = (f64::from(plot.width), f64::from(plot.height));
    if plot.waves {
        handle.draw(|scene| scene.replace(waves(1.0, width, height, true)));
    } else {
        let data: std::sync::Arc<[[f32; 2]]> = data(plot)
            .iter()
            .map(|&(x, y)| [x as f32, y as f32])
            .collect();
        handle.draw(|scene| {
            scene.push_series(zgui::canvas::Series::Points {
                data,
                to_canvas: zgui::elements::kurbo::Affine::new([
                    width, 0.0, 0.0, -height, -width, height,
                ]),
                marker: zgui::canvas::Marker::Circle { radius: RADIUS },
                fill: Some(Brush::Solid(Color::srgb(0.36, 0.62, 1.0, 1.0))),
                stroke: None,
            });
        });
    }
    let low = RwSignal::new_local(1.0_f64);
    let grab: RwSignal<Option<f32>, zgui::reactive::LocalStorage> = RwSignal::new_local(None);
    let panned = handle.clone();
    let canvas = zgui::elements::canvas()
        .class(plot.class)
        .scene(&handle)
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
                    low.update(|low| *low -= moved / width);
                    grab.set(Some(cx.position.x.0));
                    panned.set_transform(zgui::elements::kurbo::Affine::translate((
                        (1.0 - low.get_untracked()) * width,
                        0.0,
                    )));
                }
            },
        )
        .on(
            events::PointerUp,
            move |cx: &mut EventCx<'_, events::PointerUp>| {
                grab.set(None);
                cx.release_pointer();
            },
        );
    view! {
        column(class = "vector-root") {
            box(class = "plot-clip") {
                {canvas.into_view()}
            }
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
        if plot.view {
            return Box::new(viewed(plot).into_view().build(cx)) as Box<dyn Anchor>;
        }
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
