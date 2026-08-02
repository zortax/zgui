//! Whether the content a scroll brings into a port is in the frame that brought it in.
//!
//! # Why an inner port and not the window
//!
//! A window's own scroller has the viewport for a port, so everything outside it is outside the
//! surface as well: the frame cuts its damage to the surface before anything reads it, the emit
//! walk never enters what is out of view, and a row arriving from below arrives with nothing said
//! about it. A port that is a *box on the page* is the other case. Its rows are out of the port and
//! squarely on the surface, so a scroll damages them, the walk enters them on every frame while
//! they paint nothing at all, and whatever it concludes about them down there travels with them
//! into view.
//!
//! # How a frame is asked what it drew
//!
//! Each row is filled with a colour no other row uses — the green channel is the row's index — so a
//! frame's display list can be asked which rows it holds without knowing a single fragment name.
//! The renderer draws nothing and keeps the quads.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::BuildCx;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// One frame of the surface's refresh rate, rounded up past the deadline the park installs.
const FRAME: std::time::Duration = std::time::Duration::from_millis(20);

/// How many rows the list holds.
const ROWS: usize = 24;

/// How tall one row is, in CSS pixels.
const ROW: f32 = 20.0;

/// How tall the port is, in CSS pixels. Six rows fit; eighteen do not.
const PORT: f32 = 120.0;

/// What one frame put in its display list.
#[derive(Debug, Clone)]
struct Drawn {
    /// Every quad's top, height and green channel, which is what names its row.
    quads: Vec<(f32, f32, u8)>,
}

impl Drawn {
    /// The rows this frame drew, by index.
    fn rows(&self) -> Vec<usize> {
        let mut held: Vec<usize> = self
            .quads
            .iter()
            .filter(|(_, height, _)| (*height - ROW).abs() < 0.5)
            .map(|(_, _, green)| *green as usize)
            .filter(|row| *row < ROWS)
            .collect();
        held.sort_unstable();
        held.dedup();
        held
    }
}

/// The frames, in the order they were drawn.
type Log = Rc<RefCell<Vec<Drawn>>>;

/// A renderer that records each frame's quads and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go to plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Recorder {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.log.borrow_mut().push(Drawn {
            quads: scene
                .primitives
                .quads
                .iter()
                .filter_map(|quad| {
                    let color = match scene.paints.get(quad.fill.id()?) {
                        Some(zgui_scene::Paint::Solid(color)) => *color,
                        _ => return None,
                    };
                    let green = (color.components()[1] * 255.0).round() as u8;
                    Some((quad.bounds[1], quad.bounds[3], green))
                })
                .collect(),
        });
        FrameOutcome::Presented(zgui_render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        TextureHandle(0)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// A stylesheet giving every row a green channel equal to its index.
fn sheet() -> String {
    let mut css = format!(
        "root {{ display: block; width: 400px; height: 300px }}
         .port {{ display: block; width: 400px; height: {PORT}px; overflow: scroll }}
         .row {{ display: block; width: 400px; height: {ROW}px }}
        "
    );
    for row in 0..ROWS {
        css.push_str(&format!(
            ".r{row} {{ background-color: rgb(10, {row}, 10) }}\n"
        ));
    }
    css
}

/// A window holding one scroll port, smaller than the window, with [`ROWS`] coloured rows in it.
fn listing(log: &Log) -> zgui_platform_headless::Harness<Runtime> {
    let factory = Rc::clone(log);
    let css: &'static str = Box::leak(sheet().into_boxed_str());
    let handler = App::new()
        .with_title("arrival")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Recorder {
                    log: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            use zgui_view::{IntoView, View};
            let mut port = zgui_elements::column().class("port");
            for row in 0..ROWS {
                port = port.child(
                    zgui_elements::column()
                        .class("row")
                        .class(Box::leak(format!("r{row}").into_boxed_str()) as &'static str),
                );
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(port)
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// Turns the wheel `lines` lines over the port, without waiting for the glide to finish.
fn wheel(harness: &mut zgui_platform_headless::Harness<Runtime>, lines: f32) {
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
}

/// The top edge of the first row, in device pixels, which is where the whole list is.
///
/// Read off the fragments rather than off the scroll offset, because what the assertion is about is
/// where the rows were drawn and the offset is one input to that.
fn first_row_top(harness: &zgui_platform_headless::Harness<Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("one window");
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
    top.expect("the rows were laid out")
}

/// Every row lying wholly inside the port, given where the first one is.
fn covered(top: f32) -> Vec<usize> {
    (0..ROWS)
        .filter(|row| {
            let at = top + ROW * *row as f32;
            at >= -0.01 && at + ROW <= PORT + 0.01
        })
        .collect()
}

/// A row that scrolls into the port is drawn by the frame that brought it there.
///
/// The port is scrolled one detent at a time, and after every frame the rows the display list holds
/// are compared against the rows the port's own rectangle now covers. A row wholly inside the port
/// and absent from the frame is a hole on the screen: the scroll damaged the whole port, so nothing
/// downstream is going to draw it either.
#[test]
fn a_row_scrolled_into_a_port_is_in_the_frame_that_brought_it_in() {
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let mut harness = listing(&log);
    harness.settle(8);
    let mounted = covered(first_row_top(&harness));
    log.borrow_mut().clear();

    let mut arrived: Vec<usize> = Vec::new();
    let mut frames = 0_u32;
    for _ in 0..6 {
        wheel(&mut harness, 1.0);
        for _ in 0..10 {
            let before = log.borrow().len();
            harness.advance(FRAME);
            harness.pump();
            if log.borrow().len() == before {
                continue;
            }
            frames += 1;
            let top = first_row_top(&harness);
            let want = covered(top);
            let drawn = log.borrow().last().expect("a frame was drawn").rows();
            let missing: Vec<usize> = want
                .iter()
                .copied()
                .filter(|row| !drawn.contains(row))
                .collect();
            assert!(
                missing.is_empty(),
                "rows {missing:?} lie wholly inside the port with its first row at {top} and the \
                 frame that scrolled them there drew {drawn:?}"
            );
            for row in want {
                if !mounted.contains(&row) && !arrived.contains(&row) {
                    arrived.push(row);
                }
            }
        }
    }

    // The control. Every assertion above is vacuous over a port that never moved, and a port whose
    // rows were all on the screen when it opened asks nothing at all: what is under test is the
    // record a row acquires while it is hidden, so a row that was never hidden is not a witness.
    assert!(frames > 0, "the port drew no frames at all");
    assert!(
        arrived.len() >= 6,
        "only {arrived:?} arrived from outside the port, over {frames} frames, and rows \
         {mounted:?} were already in it when it opened"
    );
}
