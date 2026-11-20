//! What a scroll draws when the renderer can move the pixels it already composed.
//!
//! A scroll moves every pixel of a scrollport, so it damages the whole port, so the emit walk
//! reaches every fragment in it — 77 % of a scroll frame (`docs/perf/scroll-frame.md`). A renderer
//! that keeps its composed target does not need most of that: the pixels are one whole-pixel
//! translation from where they belong, and only the band the translation uncovers has to be drawn.
//!
//! Every claim below is a **differential against the same document scrolled the same way**, with
//! one thing changed: whether the renderer says it keeps composed pixels, or whether the document
//! is one whose port may be moved. What is measured is the area the frames of the glide were
//! damaged over. A bare assertion that "little was damaged" would be equally true of a scroll that
//! moved nothing, so the control is always a run in which the number is large.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::Surface;
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    ScrollShift, TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{Anchor, BuildCx};
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

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
.away { margin-top: 12px; margin-left: 24px; width: 300px; height: 200px }
.patterned { background-image: linear-gradient(#000000, #ffffff) }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.over { display: block; width: 400px; height: 40px; background-color: #900000;
        position: absolute; top: 10px; left: 0 }
";

/// How tall one row is in device pixels, which is what the sheet says.
const ROW: f32 = 20.0;

/// How many frames of motion each wheel turn is carried for.
const FRAMES: usize = 24;

/// The window's area in device pixels, which is what a full-surface damage set costs.
const SURFACE_AREA: i64 = 400 * 300;

/// The area every frame of a window was damaged over, summed, in device pixels.
type Damaged = Rc<Cell<i64>>;

/// A capture renderer that adds the area of every frame's damage to a sum.
struct Measured {
    /// The renderer the frames are drawn with.
    inner: zgui_testkit_scene::CaptureRenderer,
    /// The sum.
    damaged: Damaged,
}

impl Renderer for Measured {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let area = damage.area().unwrap_or(SURFACE_AREA);
        self.damaged.set(self.damaged.get() + area);
        self.inner.draw(scene, damage)
    }

    fn shifts_composed_pixels(&self) -> bool {
        self.inner.shifts_composed_pixels()
    }

    fn shift_composed(&mut self, shift: ScrollShift) {
        self.inner.shift_composed(shift);
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        self.inner.texture_sink()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(&mut self.inner)
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(&self.inner)
    }
}

/// A window holding one scroll container, with `port_class` on it, `overlay` drawn over it, and
/// `backing_class` on the box the port is composited over, with the sum of its damage.
///
/// When `shifts` is set, the renderer claims to keep the pixels it composed.
fn listing(
    rows: usize,
    port_class: &'static str,
    overlay: bool,
    shifts: bool,
    backing_class: &'static str,
) -> (Harness<Runtime>, Damaged) {
    let damaged = Damaged::default();
    let sum = Rc::clone(&damaged);
    let renderer = move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
        let inner = if shifts {
            zgui_testkit_scene::CaptureRenderer::new().shifting()
        } else {
            zgui_testkit_scene::CaptureRenderer::new()
        };
        let mut renderer = Measured {
            inner,
            damaged: Rc::clone(&sum),
        };
        renderer.configure(target);
        Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
    };
    let handler = App::new()
        .with_title("scroll-shift")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(Box::new(renderer))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
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
    (Harness::new(handler), damaged)
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

/// How much area a whole wheel turn damaged, and how far the list actually travelled.
fn turn(rows: usize, port_class: &'static str, overlay: bool, shifts: bool) -> (i64, f32) {
    backed(rows, port_class, overlay, shifts, "")
}

/// The same, over a backing the caller chooses.
fn backed(
    rows: usize,
    port_class: &'static str,
    overlay: bool,
    shifts: bool,
    backing_class: &'static str,
) -> (i64, f32) {
    let (mut harness, damaged) = listing(rows, port_class, overlay, shifts, backing_class);
    harness.settle(16);
    let before = topmost_row(&harness);
    let mark = damaged.get();
    wheel(&mut harness, 3.0);
    let travelled = before - topmost_row(&harness);
    (damaged.get() - mark, travelled)
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
            // Where it is drawn: a scrolled row keeps its rectangle when its container draws it
            // in a content space of its own.
            let border = zgui_layout::fragment::transform::placed::onto_device(
                fragment.border_box,
                fragment.transform,
                &window.placements(),
            );
            if (border.size.height.0 - ROW).abs() > 0.5 {
                continue;
            }
            top = Some(match top {
                Some(held) => held.min(border.origin.y.0),
                None => border.origin.y.0,
            });
        }
    }
    top.expect("the document has rows in it")
}

#[test]
fn a_shiftable_scroll_damages_the_band_and_not_the_port() {
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
    // The control. Without the claim the frame damages the whole port, every frame of the glide —
    // which is the cost this whole phase exists to remove.
    assert!(
        without > 400 * 280,
        "the unshifted run is the positive control and has to be expensive: {without}",
    );
    assert!(
        with_shift * 3 < without * 2,
        "a scroll whose pixels can be moved must damage substantially less than one whose cannot: \
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
    let (opaque_over_gradient, _) = backed(200, "", false, true, "patterned");
    let (control, _) = backed(200, "", false, false, "patterned");

    assert!(
        opaque_over_gradient * 3 < control * 2,
        "an opaque port hides the gradient, so what is behind it cannot reach the composite: \
         {opaque_over_gradient} against {control}",
    );
}

#[test]
fn something_drawn_over_the_port_refuses_the_shift() {
    let (clear, _) = turn(200, "", false, true);
    let (overdrawn, _) = turn(200, "", true, true);

    assert!(
        overdrawn > clear * 2,
        "the composed pixels are the composite, so a box drawn over the port would be moved with \
         the content under it: {overdrawn} against {clear}",
    );
}

/// The port a renderer is asked to move is the one on the device, wherever the list stands.
///
/// A list at the top left corner of the window has its port at the origin in both spaces, so only
/// a list away from that corner tells the container's own space from the device's. Moving the
/// pixels at the origin while the list stands elsewhere leaves the list as it was drawn, and a
/// frame that owes it only the band the move uncovered never draws it again.
#[test]
fn the_port_moved_is_where_the_list_stands() {
    let (mut harness, _) = listing(200, "away", false, true, "");
    harness.settle(16);
    wheel(&mut harness, 3.0);

    let window = harness.app().windows().first().expect("a window");
    let shift = window
        .renderer()
        .as_any()
        .and_then(|any| any.downcast_ref::<zgui_testkit_scene::CaptureRenderer>())
        .expect("the window draws through the capture renderer")
        .last_shift()
        .expect("an opaque list over a plain page scrolls by moving its pixels");
    assert_eq!(
        (shift.port.origin.x, shift.port.origin.y),
        (24, 12),
        "the port has to be the list's own rectangle on the device: {shift:?}",
    );
    assert!(
        shift.port.size.height <= 200 && shift.port.size.width <= 300,
        "the port cannot reach past the list: {shift:?}",
    );
}
