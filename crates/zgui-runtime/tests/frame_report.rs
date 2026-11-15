//! What a probe reads about the frame that has just ended.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_platform::Surface;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui_runtime::{App, AppError, FrameProbe, FrameReport, Window};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View};

/// A window with one swatch whose colour a signal decides.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .swatch { display: block; width: 30px; height: 20px; background-color: #101010 }
                   .swatch.lit { background-color: #f0f0f0 }";

/// A renderer that numbers every frame it draws in the draw-call count it reports.
struct Numbering {
    /// How many frames it has drawn, shared with the test.
    drawn: Rc<Cell<u32>>,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Numbering {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, _scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.drawn.set(self.drawn.get() + 1);
        FrameOutcome::Presented(FrameStats {
            vector_passes: 0,
            draw_calls: self.drawn.get(),
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

/// What the probe saw at the end of one frame.
struct Seen {
    /// The report the window held.
    report: Option<FrameReport>,
    /// How many frames the renderer had drawn by then.
    drawn: u32,
}

/// A probe that records the window's report at the end of every frame.
struct Recording {
    /// The renderer's frame count.
    drawn: Rc<Cell<u32>>,
    /// Every frame's record, in order.
    seen: RefCell<Vec<Seen>>,
}

impl FrameProbe for Recording {
    fn frame_ended(&self, window: &Window) {
        self.seen.borrow_mut().push(Seen {
            report: window.last_frame_report(),
            drawn: self.drawn.get(),
        });
    }
}

#[test]
fn a_probe_reads_the_report_of_the_frame_that_has_just_ended() {
    let drawn = Rc::new(Cell::new(0));
    let probe = Rc::new(Recording {
        drawn: Rc::clone(&drawn),
        seen: RefCell::new(Vec::new()),
    });
    let lit = RwSignal::new(false);
    let counter = Rc::clone(&drawn);
    let handler = App::new()
        .with_title("frame report")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_probe(Rc::clone(&probe) as Rc<dyn FrameProbe>)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Numbering {
                    drawn: Rc::clone(&counter),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::column()
                            .class("swatch")
                            .class_toggle(zgui_view::ClassName::new("lit"), move || lit.get()),
                    )
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut app = zgui_platform_headless::Harness::new(handler);
    app.settle(8);
    lit.set(true);
    app.settle(8);

    let seen = probe.seen.borrow();
    assert!(
        seen.len() >= 2,
        "two changes reached the probe {} times",
        seen.len()
    );
    let mut presented = 0;
    for (index, frame) in seen.iter().enumerate() {
        let report = frame
            .report
            .unwrap_or_else(|| panic!("frame {index} reached the probe with no report"));
        assert!(
            report.paint_elapsed <= report.elapsed,
            "frame {index} painted for {:?} of {:?}",
            report.paint_elapsed,
            report.elapsed
        );
        if let FrameOutcome::Presented(stats) = report.outcome {
            assert_eq!(
                stats.draw_calls, frame.drawn,
                "frame {index} reported the draw of an earlier frame"
            );
            presented += 1;
        }
    }
    assert!(presented >= 2, "only {presented} frames were drawn");
    let last = seen.last().and_then(|frame| frame.report);
    assert_eq!(
        app.app().windows()[0].last_frame_report(),
        last,
        "the window holds a different report than the probe read at the end of the last frame"
    );
}
