//! Every style change below the level that moves anything still has to damage a rectangle.
//!
//! These are the properties with no size and no position consequence: a colour, a transform on a
//! box that was already transformed, a stacking order, a visibility. The engine's own damage
//! reports nothing for several of them, and the ones it does report are for work this framework
//! does not do — so the translation from what the cascade changed into what the frame owes is the
//! only thing standing between a changed value and a screen that still shows the old one.
//!
//! The transform case starts from an **already transformed** element on purpose. An element gaining
//! its first transform is relaid out for other reasons, so without that precondition the case
//! passes over code that drops transform damage entirely.
//!
//! # A bit set is not a pixel damaged
//!
//! Each case asserts both: the obligation was marked, *and* the frame that serviced it was drawn
//! against a rectangle that covers the element. The second is what a counter cannot see — the
//! fragment pass is the only stage that can produce a rectangle for a fragment that still exists,
//! and a repaint obligation that does not reach it is a redraw of nothing.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use zgui_bits::DamageSet;
use zgui_geom::{Device, Rect};
use zgui_platform::Surface;
use zgui_profile::{COUNTERS_ENABLED, counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View};

/// Held for the whole of any case that reads a counter.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Takes the counter lock and zeroes every counter.
fn measuring() -> MutexGuard<'static, ()> {
    let guard = COUNTERS.lock().unwrap_or_else(|held| held.into_inner());
    counter::reset();
    guard
}

/// One element in the middle of a window, and one class per property under test.
///
/// The subject is transformed, stacked and bordered *before* any class is toggled, so that every
/// toggle changes a property the element already had rather than giving it one. A second card
/// overlaps it and sits above it, because a stacking change on an element that covers nothing and
/// is covered by nothing genuinely owes no repaint — a fixture without the overlap would be
/// asserting that the frame does work it has no reason to do.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 100px;
                          position: relative }
                   .subject { display: block; width: 120px; height: 60px;
                              background-color: rgb(16, 16, 16);
                              border: 2px solid rgb(30, 30, 30);
                              position: absolute; left: 100px; top: 100px; z-index: 1;
                              transform: rotate(5deg) }
                   .cover { display: block; width: 120px; height: 60px;
                            background-color: rgb(90, 90, 90);
                            position: absolute; left: 140px; top: 120px; z-index: 2 }
                   .rotated  { transform: rotate(25deg) }
                   .spun     { rotate: 30deg }
                   .on-top   { z-index: 9 }
                   .faded    { opacity: 0.5 }
                   .ringed   { border-color: rgb(240, 10, 10) }
                   .gone     { visibility: hidden }
                   .embossed { text-shadow: 1px 1px 0 rgb(200, 0, 0) }
                   .lit      { background-color: rgb(240, 240, 240) }";

/// What one frame was drawn against.
#[derive(Debug, Clone, Copy)]
struct Drawn {
    /// Whether the set was the whole surface.
    full: bool,
    /// The rectangle the set's own rectangles are bounded by, or nothing when it is empty.
    bounds: Option<Rect<i32, Device>>,
}

/// The frames a run was drawn against, in order.
type Log = Rc<RefCell<Vec<Drawn>>>;

/// A renderer that records the damage of every frame and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
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

    fn draw(&mut self, _scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        self.log.borrow_mut().push(Drawn {
            full: damage.is_full(),
            bounds: damage.bounds(),
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

/// Mounts one subject carrying `class` whenever `on` says so.
fn subject(
    class: &'static str,
    on: RwSignal<bool>,
    log: &Log,
) -> zgui_platform_headless::Harness<Runtime> {
    let factory = Rc::clone(log);
    let handler = App::new()
        .with_title("sub-relayout")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
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
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::column()
                            .class("subject")
                            .class_toggle(zgui_interned::ClassName::new(class), move || on.get()),
                    )
                    .child(zgui_elements::column().class("cover"))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// The subject's ink rectangle, in whole device pixels.
///
/// Found through the element that carries the class rather than by size, because half the
/// properties under test change the size of the ink and a rectangle identified by its extent would
/// then be identifying a different box after the toggle than before it.
fn ink_of_subject(harness: &zgui_platform_headless::Harness<Runtime>) -> Rect<i32, Device> {
    let window = harness.app().windows().first().expect("one window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    let mut held: Option<Rect<i32, Device>> = None;
    for key in layout.keys() {
        let Some(source) = layout.get(key).and_then(|node| node.source) else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|class| &**class == "subject")
        {
            continue;
        }
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            let rect = zgui_layout::fragment::diff::pixels(fragment.ink);
            held = Some(match held {
                Some(union) => union.union(rect),
                None => rect,
            });
        }
    }
    held.expect("the subject generated a fragment")
}

/// Whether `outer` covers every pixel of `inner`.
fn contains(outer: Rect<i32, Device>, inner: Rect<i32, Device>) -> bool {
    outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer.origin.x + outer.size.width >= inner.origin.x + inner.size.width
        && outer.origin.y + outer.size.height >= inner.origin.y + inner.size.height
}

#[test]
fn sub_relayout_style_changes_all_produce_damage() {
    let _guard = measuring();
    for class in [
        "rotated",  // transform, on an element that was already transformed
        "spun",     // rotate
        "on-top",   // z-index
        "faded",    // opacity
        "ringed",   // border-color, for which the engine reports no damage at all
        "gone",     // visibility, likewise
        "embossed", // text-shadow, which lives in the inherited-text group
        "lit",      // background-color: the commonest frame in any component library
    ] {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let toggle = RwSignal::new(false);
        let mut harness = subject(class, toggle, &log);
        harness.settle(8);
        let before = ink_of_subject(&harness);

        log.borrow_mut().clear();
        counter::reset();
        toggle.set(true);
        harness.settle(8);
        // Where it was and where it is: a transform moves the ink, and the pixels it left behind
        // are as much this frame's obligation as the ones it now covers.
        let ink = before.union(ink_of_subject(&harness));

        let frame = counter::snapshot();
        if COUNTERS_ENABLED {
            // `on-top` is the exception, and it is a real one rather than a licence: a stacking
            // change re-orders primitives that are otherwise identical, so the emit walk replays
            // every one of them and encodes none. The damage assertion below still holds for it,
            // which is what this case is about; what does not hold is that anything was *encoded*.
            assert!(
                frame.repaints >= 1 || class == "on-top",
                "`{class}` changed nothing that had to be painted again"
            );
            // `ringed` and `gone` are the other exception, and it is over-damage rather than
            // under-damage: a border colour and a visibility both relay the subtree out, which
            // neither has any reason to do. Asserted for the six that are already right, so that
            // the six cannot regress while the two are being fixed.
            if !matches!(class, "ringed" | "gone") {
                assert_eq!(
                    frame.nodes_relaid_out, 0,
                    "`{class}` relaid the document out, which it has no reason to"
                );
            }
        }

        // The half a counter cannot make. A repaint obligation that never reaches the stage which
        // produces rectangles leaves the damage empty, the frame correct by every count, and the
        // previous colour on the screen.
        let drawn = log.borrow();
        assert!(
            drawn.iter().any(|frame| frame.full
                || frame
                    .bounds
                    .is_some_and(|bounds| contains(bounds, ink))),
            "`{class}` was drawn against no rectangle covering {ink:?}: {drawn:?}"
        );
    }
}

#[test]
fn a_transform_change_rebuilds_fragments_rather_than_replaying_them() {
    let _guard = measuring();
    // A transform changes where the fragment's pixels are, so its geometry cannot be carried over
    // from the previous frame — which is a different question from whether its damage is right, and
    // the one a damage-rectangle assertion cannot ask.
    for class in ["rotated", "spun"] {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let toggle = RwSignal::new(false);
        let mut harness = subject(class, toggle, &log);
        harness.settle(8);

        counter::reset();
        toggle.set(true);
        harness.settle(8);

        if COUNTERS_ENABLED {
            assert!(
                counter::snapshot().fragments_rebuilt >= 1,
                "`{class}` replayed the geometry it had just changed"
            );
        }
    }
}
