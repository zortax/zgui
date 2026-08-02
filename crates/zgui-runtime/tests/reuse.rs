//! What a real frame reuses when one node of a real document changes.
//!
//! The damage assertions next door say how many pixels a change cost. These say whether the work
//! behind those pixels was avoided or merely scissored, which is a different question with a
//! different failure: a frame that rebuilds every box, re-derives every fragment and re-encodes
//! every primitive, and then draws the result through a small rectangle, satisfies every damage
//! bound there is while costing exactly what a full redraw costs.
//!
//! The counters are a process-wide block, so this is one test in a target of its own. Two of them
//! running beside each other would each be reading the other's frames.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_platform::Surface;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View};

/// A window with a paragraph in it and a great deal of surface around it.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 40px }
                   text { display: block; width: 300px }
                   row { display: block; width: 300px; height: 12px; background-color: #202020 }";

/// How many rows sit around the text, so that "most of the document was left alone" has a document
/// to be about.
const ROWS: usize = 24;

/// A renderer that draws nowhere and counts nothing, so the counters describe the frame alone.
struct Quiet {
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Quiet {
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
        FrameOutcome::Presented(FrameStats {
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

/// A window of rows with one line of text among them, reading `count`.
fn window(count: RwSignal<i32>) -> zgui_platform_headless::Harness<Runtime> {
    let handler = App::new()
        .with_title("reuse")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Quiet {
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
            let mut column = zgui_elements::column().class("root");
            for _ in 0..ROWS {
                column = column.child(zgui_elements::column().class("row"));
            }
            column = column.child(zgui_elements::text().child(move || count.get().to_string()));
            Box::new(column.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// How many fragments the document has, which is what "most of them" is measured against.
fn fragment_count(app: &zgui_platform_headless::Harness<Runtime>) -> u32 {
    app.app().windows()[0].layout().borrow().fragment_count()
}

/// A counter incrementing builds no box, re-derives few fragments and re-encodes almost nothing.
///
/// Every number here was the whole document before the box tree could be patched: a change of text
/// rebuilt it, so every box was built again, every fragment was created again and every primitive
/// was encoded again — and the frame then drew all of it. None of that is visible in a damage
/// rectangle, which is why it is asserted here on what the stages themselves reported.
#[test]
fn one_digit_changing_costs_the_digit_and_not_the_document() {
    let count = RwSignal::new(0);
    let mut app = window(count);
    app.settle(8);
    let fragments = fragment_count(&app);
    assert!(
        fragments >= ROWS as u32,
        "the fixture produced {fragments} fragments, which is fewer than it has rows: the \
         proportions below would mean nothing"
    );

    counter::reset();
    count.set(7);
    app.settle(8);
    let after = counter::snapshot();

    // Counters vanish in a build that neither enables the feature nor has debug assertions on, and
    // an assertion over zeroes would pass for every frame there is.
    if !COUNTERS_ENABLED {
        return;
    }

    let boxes = after.get(Counter::BoxesRebuilt);
    assert_eq!(
        boxes, 0,
        "one digit changed and {boxes} boxes were built from their elements, so the tree was \
         rebuilt rather than patched"
    );

    let rebuilt = after.get(Counter::FragmentsRebuilt);
    assert!(
        rebuilt > 0,
        "the digit changed and no fragment was recomputed at all, so nothing on the screen moved"
    );
    assert!(
        rebuilt < u64::from(fragments) / 2,
        "one digit changed and {rebuilt} of {fragments} fragments were recomputed"
    );

    let encoded = after.get(Counter::ChunksReencoded);
    let replayed = after.get(Counter::ChunksTranslated);
    assert!(
        replayed > 0,
        "not one fragment's recorded painting was replayed, so the per-fragment paint record is \
         holding nothing a frame can use"
    );
    assert!(
        encoded <= replayed,
        "one digit changed and {encoded} fragments were encoded afresh against {replayed} \
         replayed: the paint record is missing for fragments that did not change"
    );
}
