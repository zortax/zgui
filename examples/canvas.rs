//! A canvas: shapes the application draws imperatively, rasterised by the vector pipeline.
//!
//! Run it with `cargo run -p zgui-examples --example canvas --release`.
//!
//! What it is worth reading for:
//!
//! * `canvas()` is an ordinary element — a box, ordinary CSS, ordinary layout — whose *content*
//!   is a shape list the application owns rather than notation the view carries;
//! * the closure form re-runs when the signals it reads change and when the element's own box
//!   changes size, so a chart redraws on data and on resize with no wiring;
//! * the retained form is a [`CanvasHandle`] kept in the model and mutated from an event handler,
//!   which is the shape a drawing tool or a simulation wants;
//! * every shape carries its own paint — solid, gradient, or the element's `color` through
//!   `Brush::Inherited` — so one canvas mixes brushes without a stylesheet in sight.

use zgui::canvas::zgui_color::Color;
use zgui::canvas::{Brush, ShapeBuilder};
use zgui::elements::kurbo;
use zgui::elements::kurbo::Shape as _;
use zgui::prelude::*;

/// The bars the chart shows, as one signal the draw closure reads.
fn bars() -> RwSignal<Vec<f64>> {
    RwSignal::new(vec![0.8, 0.35, 0.6, 0.95, 0.5, 0.7])
}

/// A bar chart drawn from a signal: data in, shapes out, and nothing retained by hand.
#[component]
fn Chart(
    /// The values, each in `0.0..=1.0`.
    values: RwSignal<Vec<f64>>,
) -> impl IntoView {
    zgui::elements::canvas()
        .class("chart")
        .draw(move |cx| {
            let (width, height) = (f64::from(cx.size.width.0), f64::from(cx.size.height.0));
            let values = values.get();
            if values.is_empty() || width <= 0.0 {
                return;
            }
            let gap = 8.0;
            let bar = (width - gap * (values.len() as f64 - 1.0)) / values.len() as f64;
            for (index, value) in values.iter().enumerate() {
                let x = index as f64 * (bar + gap);
                let top = height * (1.0 - value);
                let rect = kurbo::Rect::new(x, top, x + bar, height);
                cx.scene.push(
                    ShapeBuilder::new(kurbo::RoundedRect::from_rect(rect, 4.0).to_path(0.1))
                        .fill(Brush::Linear {
                            start: kurbo::Point::new(x, top),
                            end: kurbo::Point::new(x, height),
                            stops: vec![
                                (0.0, Color::srgb(0.43, 0.66, 1.0, 1.0)),
                                (1.0, Color::srgb(0.20, 0.28, 0.55, 1.0)),
                            ],
                            repeating: false,
                        })
                        .build(),
                );
            }
        })
        .into_view()
}

/// A scratchpad over a retained handle: every press of "scribble" adds a stroke, "clear" empties
/// it, and nothing about the element changes — the scene is the state.
#[component]
fn Pad() -> impl IntoView {
    let handle = CanvasHandle::new();
    let strokes = RwSignal::new(0usize);

    let scribble = {
        let handle = handle.clone();
        move |_: &mut EventCx<'_, events::Click>| {
            let n = strokes.get_untracked();
            strokes.set(n + 1);
            handle.draw(|scene| {
                let phase = n as f64 * 0.7;
                let mut path = kurbo::BezPath::new();
                path.move_to((10.0, 60.0 + 40.0 * phase.sin()));
                path.curve_to(
                    (60.0, 10.0 + 30.0 * (phase * 1.3).cos()),
                    (120.0, 110.0 - 30.0 * (phase * 0.9).sin()),
                    (170.0, 60.0 + 40.0 * (phase * 1.1).cos()),
                );
                scene.push(
                    ShapeBuilder::new(path)
                        .stroke(Brush::Inherited { alpha: 0.85 }, 2.5)
                        .build(),
                );
            });
        }
    };
    let clear = {
        let handle = handle.clone();
        move |_: &mut EventCx<'_, events::Click>| {
            strokes.set(0);
            handle.draw(zgui::canvas::CanvasScene::clear);
        }
    };

    view! {
        column(class = "pad") {
            {zgui::elements::canvas().class("pad__art").scene(&handle).into_view()}
            row(class = "pad__controls") {
                control(class = "chip", tabindex = Focus::Sequential, on:click = scribble) { "scribble" }
                control(class = "chip", tabindex = Focus::Sequential, on:click = clear) { "clear" }
                label(class = "pad__count") {{move || format!("{} strokes", strokes.get())}}
            }
        }
    }
}

/// The two canvases side by side.
#[component]
fn Board() -> impl IntoView {
    let values = bars();
    let shuffle = move |_: &mut EventCx<'_, events::Click>| {
        values.update(|values| values.rotate_left(1));
    };
    view! {
        column(class = "board") {
            label(class = "board__title") {"Canvas"}
            row(class = "board__row") {
                column(class = "cell") {
                    Chart(values = values)
                    control(class = "chip", tabindex = Focus::Sequential, on:click = shuffle) { "shuffle" }
                }
                Pad()
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .board {
        align-items: center;
        gap: 20px;
        padding: 28px 36px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
    }

    .board__title { font-size: 13px; letter-spacing: 2px; color: #7d879b; }
    .board__row { gap: 28px; align-items: flex-start; }
    .cell { align-items: center; gap: 12px; }

    .chart { width: 260px; height: 150px; }

    .pad { align-items: center; gap: 12px; }
    /* The strokes use the inherited brush, so this is what colours them. */
    .pad__art { width: 180px; height: 120px; color: #6ea8ff; border: 1px solid #262b36; }
    .pad__art:hover { color: #ffb648; }
    .pad__controls { gap: 8px; align-items: center; }
    .pad__count { font-size: 12px; color: #7d879b; }

    .chip {
        padding: 4px 10px;
        border-radius: 8px;
        border: 1px solid #2c3342;
        background-color: #202634;
        font-size: 12px;
    }
    .chip:hover { background-color: #2a3145; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Canvas")
        .with_title("Canvas")
        .with_size(640.0, 420.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Board() })
}
