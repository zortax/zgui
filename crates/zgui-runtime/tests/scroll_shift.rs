//! What a scroll draws when the renderer can move the pixels it already composed.
//!
//! A scroll moves every pixel of a scrollport, so it damages the whole port, so the emit walk
//! reaches every fragment in it — 77 % of a scroll frame (`docs/perf/scroll-frame.md`). A renderer
//! that keeps its composed target does not need most of that: the pixels are one whole-pixel
//! translation from where they belong, and only the band the translation uncovers has to be drawn.
//!
//! Every claim below is a **differential against the same document scrolled the same way**, with
//! one thing changed: whether the renderer says it keeps composed pixels, or whether the document
//! is one whose port may be moved. A bare assertion that "few primitives were emitted" would be
//! equally true of a scroll that moved nothing, so the control is always a run in which the number
//! is large.
//!
//! Its own test target because the counters are one process-wide block.

mod support;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_testkit_scene::counters::Recording;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::{RenderTarget, Renderer};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{Anchor, BuildCx};

/// A list of fixed-height rows inside a scrollport a fraction of their height.
///
/// The port's own background is opaque, which is one of the two things that makes its pixels
/// movable: the composite inside it is the content's alone, so translating it is translating the
/// content. `.bare` is the same port without one.
const CSS: &str = "
root { display: block; width: 400px; height: 300px; background-color: #ffffff }
.port { display: block; width: 400px; height: 280px; overflow: scroll;
        background-color: #101010 }
.bare { background-color: rgba(0, 0, 0, 0.5) }
.patterned { background-image: linear-gradient(#000000, #ffffff) }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.over { display: block; width: 400px; height: 40px; background-color: #900000;
        position: absolute; top: 10px; left: 0 }
";

/// How tall one row is in device pixels, which is what the sheet says.
const ROW: f32 = 20.0;

/// How many frames of motion each wheel turn is carried for.
const FRAMES: usize = 24;

/// What `App::with_renderer` takes.
type Factory = Box<dyn Fn(&Arc<dyn Surface>, RenderTarget) -> Result<Box<dyn Renderer>, AppError>>;

/// A renderer that records the display list and claims to keep the pixels it composed.
fn shifting(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new().shifting();
    renderer.configure(target);
    Ok(Box::new(renderer))
}

/// The same renderer without that claim: the control for every measurement here.
fn plain(_surface: &Arc<dyn Surface>, target: RenderTarget) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    renderer.configure(target);
    Ok(Box::new(renderer))
}

use std::sync::Arc;

/// A window holding one scroll container, with `port_class` on it, `overlay` drawn over it, and
/// `backing_class` on the box the port is composited over.
fn listing(
    rows: usize,
    port_class: &'static str,
    overlay: bool,
    shifts: bool,
    backing_class: &'static str,
) -> Harness<Runtime> {
    let renderer: Factory = if shifts {
        Box::new(shifting)
    } else {
        Box::new(plain)
    };
    let handler = App::new()
        .with_title("scroll-shift")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(renderer)
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            use zgui_view::{IntoView, View};
            let mut port = zgui_elements::column().class("port").class(port_class);
            for _ in 0..rows {
                port = port.child(zgui_elements::column().class("row"));
            }
            let mut root = zgui_elements::column()
                .class("root")
                .class(backing_class)
                .child(port);
            if overlay {
                root = root.child(zgui_elements::column().class("over"));
            }
            Box::new(root.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// Turns the wheel over the container and carries the detent to its destination.
fn wheel(harness: &mut Harness<Runtime>, lines: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..FRAMES {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    harness.settle(4);
}

/// How many primitives a whole wheel turn emitted, and how far the list actually travelled.
fn turn(rows: usize, port_class: &'static str, overlay: bool, shifts: bool) -> (u64, f32) {
    backed(rows, port_class, overlay, shifts, "")
}

/// The same, over a backing the caller chooses.
fn backed(
    rows: usize,
    port_class: &'static str,
    overlay: bool,
    shifts: bool,
    backing_class: &'static str,
) -> (u64, f32) {
    let mut harness = listing(rows, port_class, overlay, shifts, backing_class);
    harness.settle(16);
    let before = topmost_row(&harness);
    let mark = zgui_profile::counter::snapshot();
    wheel(&mut harness, 3.0);
    let moved = mark.delta(&zgui_profile::counter::snapshot());
    let travelled = before - topmost_row(&harness);
    (moved.primitives_emitted, travelled)
}

/// The top edge of the topmost row, in device pixels.
fn topmost_row(harness: &Harness<Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let mut top: Option<f32> = None;
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if matches!(fragment.kind, zgui_layout::FragmentKind::Scrollbar { .. }) {
                continue;
            }
            if (fragment.border_box.size.height.0 - ROW).abs() > 0.5 {
                continue;
            }
            top = Some(match top {
                Some(held) => held.min(fragment.border_box.origin.y.0),
                None => fragment.border_box.origin.y.0,
            });
        }
    }
    top.expect("the document has rows in it")
}

#[test]
fn a_shiftable_scroll_emits_the_band_and_not_the_port() {
    let _recording = Recording::begin();

    let (with_shift, travelled) = turn(200, "", false, true);
    let (without, control_travel) = turn(200, "", false, false);

    assert!(
        travelled > ROW,
        "the list has to actually move, or every count below is a count of nothing: {travelled}",
    );
    assert!(
        (travelled - control_travel).abs() < 0.5,
        "both runs must scroll the same distance or the counts are not comparable: \
         {travelled} against {control_travel}",
    );
    // The control. Without the claim the frame damages the whole port and emits it, every frame of
    // the glide — which is the cost this whole phase exists to remove.
    assert!(
        without > 100,
        "the unshifted run is the positive control and has to be expensive: {without}",
    );
    assert!(
        with_shift * 2 < without,
        "a scroll whose pixels can be moved must emit substantially less than one whose cannot: \
         {with_shift} against {without}",
    );
    assert!(
        without > 0 && with_shift > 0,
        "both runs must draw something: {with_shift} against {without}",
    );
}

/// A port need not be opaque itself. What has to be true is that whatever shows through it is one
/// flat colour, because translating a flat colour is the identity — so a translucent list over a
/// plain page is moved, and the same list over a gradient is not.
#[test]
fn what_shows_through_the_port_has_to_be_flat() {
    let _recording = Recording::begin();

    let (over_flat, _) = backed(200, "bare", false, true, "");
    let (over_gradient, _) = backed(200, "bare", false, true, "patterned");

    assert!(
        over_gradient > over_flat * 2,
        "a gradient behind a translucent port is in the composite and does not scroll, so the \
         pixels must not be moved: {over_gradient} against {over_flat}",
    );
}

/// And the container's own opaque background is enough on its own, whatever is behind it.
#[test]
fn an_opaque_port_needs_nothing_of_what_is_behind_it() {
    let _recording = Recording::begin();

    let (opaque_over_gradient, _) = backed(200, "", false, true, "patterned");
    let (control, _) = backed(200, "", false, false, "patterned");

    assert!(
        opaque_over_gradient * 2 < control,
        "an opaque port hides the gradient, so what is behind it cannot reach the composite: \
         {opaque_over_gradient} against {control}",
    );
}

#[test]
fn something_drawn_over_the_port_refuses_the_shift() {
    let _recording = Recording::begin();

    let (clear, _) = turn(200, "", false, true);
    let (overdrawn, _) = turn(200, "", true, true);

    assert!(
        overdrawn > clear * 2,
        "the composed pixels are the composite, so a box drawn over the port would be moved with \
         the content under it: {overdrawn} against {clear}",
    );
}
