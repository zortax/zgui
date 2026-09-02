//! A shader of the application's own, over an element the framework laid out.
//!
//! Run it with `cargo run -p zgui-examples --example shader --release`.
//!
//! What it is worth reading for:
//!
//! * an effect is declared once with [`shader!`](zgui::shader::shader) and compiled while *this*
//!   example is built — WGSL that does not parse fails `cargo build`, with the shader front end's
//!   own message and the line it was on;
//! * the shader writes one function. The vertex stage, the clip chain, the transform and the blend
//!   state belong to the framework, so a trail inside a rounded, scrolled card is clipped exactly
//!   as that card's background is;
//! * `Params` is written twice — once in WGSL and once in Rust — and the two are compared field by
//!   field when the effect is registered, so a field added on one side alone is an error rather
//!   than a rectangle full of the wrong numbers;
//! * the effect declares that it reads `Time`, which is what makes the window keep drawing. An
//!   effect that declares nothing costs what a background costs;
//! * the row below is ordinary boxes with an ordinary gradient. Two carry `--zgui-shape`, so a
//!   shader decides which of their pixels are inside while the cascade goes on deciding what fills
//!   them — a shape that is *data*, which is what a coverage effect is for. The third is cut with
//!   `--zgui-corner-shape`, which is the engine's own and needs no shader at all.

use zgui::geom::{Css, CssPx, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view::FrameHandle;

/// What the trail is drawn with, as Rust has it. The WGSL below declares the same four floats.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Trail {
    /// Where the pointer is, in the element's own device pixels.
    head: [f32; 2],
    /// How far the glow reaches, in device pixels.
    reach: f32,
    /// The hue, in turns.
    hue: f32,
}

/// A glow that follows the pointer and breathes.
static TRAIL: ShaderEffect<Trail> = shader! {
    name: "cursor-trail",
    mode: Paint,
    params: Trail,
    reads: [Time],
    source: r#"
        struct Params {
            head: vec2<f32>,
            reach: f32,
            hue: f32,
        }

        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            let reach = max(params.reach, 1.0);
            let pulse = 0.75 + 0.25 * sin(in.time * 2.5);
            let falloff = exp(-distance(in.local, params.head) / reach);
            // A second, wider and slower ring, so the trail has a body rather than one edge.
            let halo = 0.35 * exp(-distance(in.local, params.head) / (reach * 3.0));
            let alpha = saturate((falloff + halo) * pulse);
            let hue = params.hue + 0.12 * falloff;
            return premultiplied(hsl(hue, 0.85, 0.62), alpha);
        }
    "#,
};

/// A progress arc: the shader decides which pixels are inside, and CSS goes on deciding the fill.
///
/// A coverage effect earns its keep where the shape is *data*. A smoothed corner is not — it is a
/// property of the box, and the engine has `--zgui-corner-shape` for it, which reaches the border,
/// the shadow, the outline and the clip a box gives its children as well. What no property can say
/// is a ring whose sweep comes out of the cascade and changes as the value it shows does.
static ARC: ShaderEffect<Arc> = shader! {
    name: "progress-arc",
    mode: Coverage,
    params: Arc,
    source: r#"
        struct Params {
            // How much of the ring is filled, from zero to one.
            fraction: f32,
            // How thick the ring is, as a fraction of its radius.
            thickness: f32,
        }

        fn coverage(in: ShaderInput, params: Params) -> f32 {
            let half = in.size * 0.5;
            let from_centre = in.local - half;
            let radius = min(half.x, half.y);
            let band = radius * clamp(params.thickness, 0.02, 1.0);

            // The ring: everything within half a band of the circle through the middle of it.
            let ring = abs(length(from_centre) - (radius - band * 0.5));
            let inside = 1.0 - smoothstep(band * 0.5 - 1.0, band * 0.5 + 1.0, ring);

            // The sweep, clockwise from twelve o'clock.
            let angle = atan2(from_centre.x, -from_centre.y);
            let turned = select(angle, angle + 2.0 * 3.141592653589793, angle < 0.0)
                / (2.0 * 3.141592653589793);
            let swept = step(turned, clamp(params.fraction, 0.0, 1.0));

            // The leading end is rounded off, so a part-filled ring does not stop on a hard radius.
            let cap = 1.0 - smoothstep(0.0, 0.02, turned - params.fraction);
            return inside * max(swept, cap * inside);
        }
    "#,
};

/// What the arc is drawn with. The style sheet writes `--progress-arc-fraction`.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Arc {
    /// How much of the ring is filled, from zero to one.
    fraction: f32,
    /// How thick the ring is, as a fraction of its radius.
    thickness: f32,
}

/// A glass lens over whatever is behind it.
///
/// A *filter* effect rather than a paint one: it is handed the content already drawn beneath it and
/// returns what replaces it. That is the case an application shader is genuinely for — a fixed
/// vocabulary can offer `blur()` and `saturate()`, but not whatever refraction this lens invents.
///
/// It declares no reach, and that is not an oversight: every sample it takes is *inside* its own
/// circle, because a magnifier pulls its reading toward the middle. An effect that read outside
/// would have to say how far, or a partial redraw would feed it its own previous output.
static LENS: ShaderEffect<Glass> = shader! {
    name: "lens",
    mode: Filter,
    params: Glass,
    source: r#"
        struct Params {
            // How hard the glass bends what it reads, from zero to about one.
            strength: f32,
            // How bright the rim is.
            rim: f32,
        }

        fn apply(
            in: ShaderInput,
            params: Params,
            beneath: texture_2d<f32>,
            beneath_sampler: sampler,
            region: FilterSource,
        ) -> vec4<f32> {
            let half = in.size * 0.5;
            let from_centre = in.local - half;
            let radius = max(min(half.x, half.y), 1.0);
            let d = length(from_centre) / radius;
            if d > 1.0 {
                // The box is square and the lens is round, so the corners are left alone.
                return source_at(beneath, beneath_sampler, region, in.local);
            }

            // A dome: nearly flat in the middle and steepening toward the rim, which is what makes
            // the edge of a glass bead smear what is behind it while the centre stays legible.
            let edge = smoothstep(0.45, 1.0, d);
            let squeeze = 1.0 - clamp(params.strength, 0.0, 1.0) * (0.08 + 0.55 * edge * edge);
            var color = source_at(beneath, beneath_sampler, region, half + from_centre * squeeze);

            // The rim, and a faint lift across the whole bead so it reads as glass sitting on the
            // page rather than as a hole cut in it.
            let ring = smoothstep(0.82, 0.97, d) * (1.0 - smoothstep(0.97, 1.0, d));
            let lift = 0.05 * (1.0 - d * d);
            return color + vec4<f32>(1.0) * (ring * params.rim + lift);
        }
    "#,
};

/// What the lens is drawn with. The style sheet writes `--lens-strength` and `--lens-rim`.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Glass {
    /// How hard the glass bends what it reads.
    strength: f32,
    /// How bright the rim is.
    rim: f32,
}

/// The pane the trail is drawn over.
struct Glow {
    /// The effect, and the parameters it draws with.
    handle: ShaderHandle<Trail>,
    /// Where the pointer is, as the event said it: on the surface, in CSS pixels.
    ///
    /// Two conversions are owed before a shader can read it, and both belong in `paint`: an event
    /// speaks CSS pixels and a shader is drawn in device pixels, and an event speaks the surface's
    /// coordinates while an effect is written against its own box's.
    pointer: Point<CssPx, Css>,
}

impl CustomElement for Glow {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: cx.known_width.unwrap_or(320.0 * cx.scale),
            height: cx.known_height.unwrap_or(180.0 * cx.scale),
            ..CustomMeasured::default()
        }
    }

    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        let size = painter.size();
        let whole = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(size.width, size.height),
        );
        // Both conversions the stored position owes: into device pixels, which is what a shader
        // is drawn in, and into this pane's own coordinates, which is what an effect is written
        // against. Only here is the scale and the pane's corner known.
        let scale = painter.scale();
        let origin = painter.origin();
        self.handle.set_params(Trail {
            head: [
                self.pointer.x.0 * scale - origin.x.0,
                self.pointer.y.0 * scale - origin.y.0,
            ],
            reach: 42.0 * scale,
            hue: 0.58,
        });
        // One instance, in the same arena a background lives in, drawn in the same pass.
        painter.effect(whole, 14.0 * painter.scale(), &self.handle);
    }
}

/// The pane, its shader, and the pointer that drives it.
#[component]
fn Pane() -> impl IntoView {
    let trail = TRAIL.register();
    // Registered so that the style sheet's `--zgui-shape: progress-arc` resolves to something.
    // Nothing else here names it: the rings are ordinary boxes with an ordinary gradient.
    let _arc = ARC.register();
    // The lens is a backdrop filter, so it is named by the style sheet rather than reached from
    // here; registering is what makes that name resolve to something.
    let _lens = LENS.register();

    // Where the lens sits, in the window's own CSS pixels, and where it was grabbed within itself.
    let lens_at = RwSignal::new_local((300.0f32, 210.0f32));
    let grab: RwSignal<Option<(f32, f32)>, LocalStorage> = RwSignal::new_local(None);
    let (glow, handle) = zgui::custom::custom(Glow {
        handle: trail.clone(),
        pointer: Point::new(CssPx(0.0), CssPx(0.0)),
    });

    // The effect reads the clock, so something has to keep asking for frames. A frame callback
    // that reschedules itself is exactly that, and the pane repaints from it — an effect writes no
    // fragment of its own accord, so nothing else would damage its rectangle.
    let pump: RwSignal<Option<FrameHandle>, LocalStorage> = RwSignal::new_local(None);
    fn tick(element: CustomHandle<Glow>, pump: RwSignal<Option<FrameHandle>, LocalStorage>) {
        let again = element.clone();
        pump.set(Some(request_frame(move |_| {
            again.repaint();
            tick(again, pump);
        })));
    }
    tick(handle.clone(), pump);

    let moved = {
        let element = handle.clone();
        move |cx: &mut EventCx<'_, events::PointerMove>| {
            // An event says where the pointer is on the surface, and only `paint` knows where this
            // pane ended up, so the position is stored as it arrived and converted there.
            let at = cx.position;
            element.update(|glow| glow.pointer = at);
            element.repaint();
        }
    };

    let glow = glow.class("glow").on(events::PointerMove, moved);

    let lens_down = move |cx: &mut EventCx<'_, events::PointerDown>| {
        let (x, y) = lens_at.get_untracked();
        // Where inside the lens it was taken hold of, so it does not jump to the pointer.
        grab.set(Some((cx.position.x.0 - x, cx.position.y.0 - y)));
        cx.capture_pointer();
    };
    let lens_move = move |cx: &mut EventCx<'_, events::PointerMove>| {
        if let Some((dx, dy)) = grab.get_untracked() {
            lens_at.set((cx.position.x.0 - dx, cx.position.y.0 - dy));
        }
    };
    let lens_up = move |cx: &mut EventCx<'_, events::PointerUp>| {
        grab.set(None);
        cx.release_pointer();
    };

    view! {
        box(class = "stage") {
        column(class = "pane") {
            label(class = "pane__title") {"Application shader"}
            {glow.into_view()}
            label(class = "pane__hint") {"move the pointer over the pane"}
            row(class = "pane__cards") {
                box(class = "ring ring--third") {}
                box(class = "ring ring--most") {}
                box(class = "card card--squircle") { label {"squircle"} }
            }
        }
        box(
            class = "lens",
            style:left = move || Some(format!("{}px", lens_at.get().0)),
            style:top = move || Some(format!("{}px", lens_at.get().1)),
            on:pointer_down = lens_down,
            on:pointer_move = lens_move,
            on:pointer_up = lens_up,
        ) {}
        }
    }
}

/// How it looks. The glow pane is an ordinary box: the sheet decides its size and its corner, and
/// the shader draws inside whatever that turns out to be.
const SHEET: &str = css!(
    ":root {
        background-color: #0e1016;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .pane {
        align-items: center;
        gap: 18px;
        padding: 28px 32px;
        border-radius: 18px;
        border: 1px solid #232833;
        background-color: #151922;
    }

    .pane__title { font-size: 13px; letter-spacing: 2px; color: #7d879b; }

    .glow {
        width: 360px;
        height: 200px;
        border-radius: 14px;
        background-color: #0b0d13;
    }

    .pane__hint { font-size: 11px; color: #59627a; }

    /* The lens is positioned against this, which fills the window. */
    .stage {
        position: relative;
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    /* No background of its own: everything it shows is what was already drawn beneath it, read
       back through the shader the property names. */
    .lens {
        position: absolute;
        width: 150px;
        height: 150px;
        border-radius: 75px;
        --zgui-backdrop-filter: lens;
        --lens-strength: 0.85;
        --lens-rim: 0.5;
    }

    .pane__cards { gap: 12px; }

    .card {
        width: 120px;
        height: 72px;
        align-items: center;
        justify-content: center;
        font-size: 12px;
        color: #0e1016;
        background: linear-gradient(140deg, #8fc0ff, #6ea8ff);
    }

    /* The engine's own corner shape: no shader, and it reaches the border and the clip too. */
    .card--squircle { border-radius: 26px; --zgui-corner-shape: squircle }

    /* The shader decides which pixels are inside; the gradient goes on filling them. */
    .ring {
        width: 72px;
        height: 72px;
        background: linear-gradient(140deg, #f9e2af, #f38ba8);
        --zgui-shape: progress-arc;
        --progress-arc-thickness: 0.28;
    }
    .ring--third { --progress-arc-fraction: 0.33 }
    .ring--most { --progress-arc-fraction: 0.85 }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Shader")
        .with_title("Application shader")
        .with_size(520.0, 500.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Pane() })
}
