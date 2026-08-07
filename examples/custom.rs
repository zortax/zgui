//! A custom element: a widget the application implements rather than composes.
//!
//! Run it with `cargo run -p zgui-examples --example custom --release`.
//!
//! What it is worth reading for:
//!
//! * a [`CustomElement`] is a trait, not a texture and not a path list: it measures itself,
//!   places its own children, and paints through the engine's own pipelines — the filled track
//!   below is a quad exactly as a background is, and the needle is one vector path;
//! * the element keeps its state (`value`) across frames, and the [`CustomHandle`] is how event
//!   handlers reach it: mutate with `update`, then say what that owes with `repaint` — the
//!   element's recorded primitives replay untouched on every frame in between;
//! * its child is an ordinary element — a label, styled by ordinary CSS — that the gauge lays
//!   out and parks wherever its needle points, which no built-in layout says;
//! * CSS still owns the outside: the element answers its content size, and width, padding and
//!   the box around it are the stylesheet's business.

use zgui::canvas::zgui_color::Color;
use zgui::custom::Space;
use zgui::elements::kurbo;
use zgui::geom::{DevicePx, Point, Rect, Size};
use zgui::prelude::*;

/// A gauge: a track, a proportional fill, tick marks, a needle, and a label it drags along.
struct Gauge {
    /// Where the needle points, in `0.0..=1.0`.
    value: f32,
}

impl CustomElement for Gauge {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        let width = cx.known_width.unwrap_or(260.0 * cx.scale);
        let height = 64.0 * cx.scale;
        if cx.final_pass && cx.access.child_count() > 0 {
            // The label sizes itself — it is ordinary text — and the gauge decides where it
            // goes: centred over the needle, clamped inside the box.
            let measured = cx.access.measure_child(
                0,
                (None, None),
                (Space::MaxContent, Space::MaxContent),
            );
            cx.access.layout_child(
                0,
                (Some(measured.width), Some(measured.height)),
                (Space::Definite(measured.width), Space::Definite(measured.height)),
            );
            let needle = width * self.value;
            let x = (needle - measured.width / 2.0).clamp(0.0, width - measured.width);
            cx.access.place_child(0, x, 0.0);
        }
        CustomMeasured {
            width,
            height,
            ..CustomMeasured::default()
        }
    }

    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        let scale = painter.scale();
        let size = painter.size();
        let (width, height) = (size.width.0, size.height.0);
        let track_top = height - 22.0 * scale;
        let track = |left: f32, right: f32| {
            Rect::new(
                Point::new(DevicePx(left), DevicePx(track_top)),
                Size::new(DevicePx(right - left), DevicePx(10.0 * scale)),
            )
        };

        // The track and its fill: two quads, the cheap path, exactly what a background costs.
        painter.fill(track(0.0, width), 5.0 * scale, Color::srgb(0.16, 0.18, 0.24, 1.0));
        let needle = width * self.value;
        painter.fill(track(0.0, needle), 5.0 * scale, painter.current_color());

        // Tick marks: thin quads under the track, fading with distance from the fill.
        for tick in 0..=10 {
            let x = width * tick as f32 / 10.0;
            let lit = x <= needle;
            painter.fill(
                Rect::new(
                    Point::new(DevicePx(x - scale / 2.0), DevicePx(track_top + 14.0 * scale)),
                    Size::new(DevicePx(scale), DevicePx(6.0 * scale)),
                ),
                0.0,
                if lit {
                    painter.current_color()
                } else {
                    Color::srgb(0.3, 0.33, 0.4, 1.0)
                },
            );
        }

        // The needle: one vector path, for the geometry a quad cannot say.
        let tip = f64::from(needle);
        let base = f64::from(track_top);
        let mut path = kurbo::BezPath::new();
        path.move_to((tip, base - 4.0 * f64::from(scale)));
        path.line_to((tip - 5.0 * f64::from(scale), base - 14.0 * f64::from(scale)));
        path.line_to((tip + 5.0 * f64::from(scale), base - 14.0 * f64::from(scale)));
        path.close_path();
        painter.fill_path(path, Color::srgb(1.0, 0.72, 0.28, 1.0));
    }
}

/// The gauge with its controls beside it.
#[component]
fn Dial() -> impl IntoView {
    let (gauge, handle) = zgui::custom::custom(Gauge { value: 0.35 });
    let percent = RwSignal::new(35u32);

    let nudge = move |by: f32| {
        let handle = handle.clone();
        move |_: &mut EventCx<'_, events::Click>| {
            let value = handle.update(|gauge| {
                gauge.value = (gauge.value + by).clamp(0.0, 1.0);
                gauge.value
            });
            percent.set((value * 100.0).round() as u32);
            // The mutation owes a relayout, not only a repaint: the label the element places
            // follows the needle, and where a child sits is layout's to keep.
            handle.relayout();
        }
    };
    let down = nudge(-0.1);
    let up = nudge(0.1);

    let gauge = gauge.class("gauge").child(
        zgui::elements::label()
            .class("gauge__readout")
            .child(move || format!("{}%", percent.get())),
    );

    view! {
        column(class = "dial") {
            label(class = "dial__title") {"Custom element"}
            {gauge.into_view()}
            row(class = "dial__controls") {
                control(class = "chip", tabindex = Focus::Sequential, on:click = down) { "−10" }
                control(class = "chip", tabindex = Focus::Sequential, on:click = up) { "+10" }
            }
        }
    }
}

/// How it looks. The gauge itself takes ordinary CSS: its width is the sheet's, its colour is
/// `color` — which is what `painter.current_color()` and the hover rule below meet on.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .dial {
        align-items: center;
        gap: 18px;
        padding: 28px 36px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
    }

    .dial__title { font-size: 13px; letter-spacing: 2px; color: #7d879b; }

    .gauge { width: 320px; color: #6ea8ff; }
    .gauge:hover { color: #8fc0ff; }
    .gauge__readout { font-size: 13px; color: #aeb9cc; }

    .dial__controls { gap: 8px; }
    .chip {
        padding: 4px 12px;
        border-radius: 8px;
        border: 1px solid #2c3342;
        background-color: #202634;
        font-size: 12px;
    }
    .chip:hover { background-color: #2a3145; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Custom")
        .with_title("Custom element")
        .with_size(480.0, 300.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Dial() })
}
