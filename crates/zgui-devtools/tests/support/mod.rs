//! A window with the inspector in it, driven headlessly.
//!
//! Shared by every integration test here rather than copied into each, because several of them
//! have to run in a process of their own: the latency ring is process-global, so a test that
//! counts what a frame wrote into it cannot share a binary with one that opens a second window.

#![allow(
    dead_code,
    reason = "each test target uses the part of this it needs, and a target that used all of it \
              would be one test file rather than several"
)]

use std::time::Duration;

use zgui::geom::{Css, CssPx, Device, DevicePx, Point, Rect, Size};
use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui::vocab::{
    KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerEvent, Timestamp,
};
use zgui::{component, view};
#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use zgui_devtools::{DevTools, Inspector, InspectorProps};
use zgui_platform_headless::Harness;

/// The page the inspector is pointed at: `rows` rows, and one box with a size nothing else has.
#[component]
pub(crate) fn Page(
    /// How many rows to draw under the target box, which is what makes the document taller than
    /// the window or shorter than it.
    rows: usize,
) -> impl IntoView {
    view! {
        column(class = "page") {
            box(class = "target")
            control(class = "anchor", a11y:label = "Anchor")
            for row in move || (0..rows).collect::<Vec<_>>(), key = |row: &usize| *row {
                row(class = "row") {
                    text() {{format!("row {row}")}}
                }
            }
        }
    }
}

/// What the page looks like, and the inspector's own sheet beside it.
pub(crate) fn sheet() -> String {
    format!(
        "{}\n{}",
        "root { background-color: #101010 }
         .page { padding: 20px }
         .row { height: 48px }
         .target { width: 120px; height: 48px; padding: 7px; border: 3px solid #ff0000 }",
        zgui_devtools::SHEET
    )
}

/// A renderer that accepts a frame and holds nothing, so the test needs no graphics device.
pub(crate) struct Blind {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where uploads go.
    sink: Sink,
}

/// A texture sink that accepts everything and keeps nothing.
pub(crate) struct Sink;

impl zgui::atlas::TextureSink for Sink {
    fn create_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _size: Size<i32, zgui::geom::Device>,
        _format: zgui::atlas::TextureFormat,
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _bounds: zgui::geom::Rect<i32, zgui::geom::Device>,
        _format: zgui::atlas::TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: zgui::atlas::TextureId) {}
}

impl Renderer for Blind {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        _scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(1)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.sink
    }
}

/// Opens an 800x600 window with a two-element page and the inspector in it.
pub(crate) fn opened(tools: DevTools) -> Harness<Runtime> {
    sized(tools, Size::new(DevicePx(800.0), DevicePx(600.0)), 0)
}

/// Opens a window of `size` with `rows` rows under the target box.
pub(crate) fn sized(
    tools: DevTools,
    size: Size<DevicePx, Device>,
    rows: usize,
) -> Harness<Runtime> {
    let runtime = zgui::runtime::App::new()
        .with_title("inspector")
        .with_size(size.width.0, size.height.0)
        .with_stylesheet(sheet())
        .with_probe(tools.probe())
        .with_renderer(Box::new(
            |_: &std::sync::Arc<dyn Surface>, target: RenderTarget| {
                Ok::<_, AppError>(Box::new(Blind {
                    target: Some(target),
                    sink: Sink,
                }) as Box<dyn Renderer>)
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                view! { Inspector(tools = tools) {Page(rows = rows)} }
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(runtime);
    harness.deliver_to_first(SurfaceEvent::Resized(size));
    harness.settle(256);
    harness
}

/// Resizes the window and lets it settle.
pub(crate) fn resize(harness: &mut Harness<Runtime>, size: Size<DevicePx, Device>) {
    harness.deliver_to_first(SurfaceEvent::Resized(size));
    harness.settle(256);
}

/// Runs `turns` refresh intervals, drawing whatever each of them wants.
///
/// Not `settle`: with the panel open the inspector publishes what a frame did *from* that frame, so
/// the panel's content is always one frame behind the window and a settle would be waiting for a
/// window that has one more frame to run. A bounded number of turns is what a real refresh gives
/// it, and it is also what makes a runaway visible — a window that never stopped would show up as
/// an assertion about content that never arrived rather than as a hang.
pub(crate) fn run(harness: &mut Harness<Runtime>, turns: usize) {
    for _ in 0..turns {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
}

/// Runs `turns` refresh intervals and reports how many of them drew a frame.
pub(crate) fn frames_over(harness: &mut Harness<Runtime>, turns: usize) -> u64 {
    let mut frames = 0;
    for _ in 0..turns {
        harness.advance(Duration::from_micros(16_667));
        frames += harness.pump();
    }
    frames
}

/// A key press, as the platform delivers it.
pub(crate) fn key(event: KeyEvent) -> SurfaceEvent {
    SurfaceEvent::Key {
        state: KeyState::Pressed,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// The <kbd>F12</kbd> press that opens the inspector.
pub(crate) fn f12() -> SurfaceEvent {
    key(KeyEvent::named(
        NamedKey::F12,
        PhysicalKey::Code(zgui::vocab::KeyCode::F12),
    ))
}

/// A pointer move to `at`.
pub(crate) fn moved(at: Point<CssPx, Css>) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// Puts focus inside the application by pressing <kbd>Tab</kbd>.
pub(crate) fn focus_something(harness: &mut Harness<Runtime>) {
    harness.deliver_to_first(SurfaceEvent::Focused(true));
    harness.settle(64);
    harness.deliver_to_first(key(KeyEvent::named(
        NamedKey::Tab,
        PhysicalKey::Code(zgui::vocab::KeyCode::Tab),
    )));
    harness.settle(64);
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Released,
        event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(zgui::vocab::KeyCode::Tab)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(64);
}

/// Whether anything at all in the window holds focus.
pub(crate) fn anything_focused(harness: &Harness<Runtime>) -> bool {
    use zgui::reactive::prelude::GetUntracked;
    harness.app().windows()[0]
        .host()
        .focused()
        .get_untracked()
        .is_some()
}

/// The border box of the first fragment of the element carrying `class`.
///
/// # Panics
///
/// Panics when nothing in the document carries the class, because every caller is about to assert
/// something about where it is.
pub(crate) fn box_of(harness: &Harness<Runtime>, class: &str) -> Rect<DevicePx, Device> {
    find_box(harness, class).unwrap_or_else(|| panic!("nothing in the document carries `{class}`"))
}

/// The same, answering `None` rather than panicking when nothing carries the class.
pub(crate) fn find_box(harness: &Harness<Runtime>, class: &str) -> Option<Rect<DevicePx, Device>> {
    let window = harness.app().windows().first().expect("a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let Some(source) = layout.node(key).source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| held.0.as_ref() == class)
        {
            continue;
        }
        if let Some(&fragment) = layout.fragments_of_box(key).first()
            && let Some(fragment) = layout.fragment(fragment)
        {
            return Some(fragment.border_box);
        }
    }
    None
}

/// How many boxes the document was laid out into.
pub(crate) fn boxes(harness: &Harness<Runtime>) -> usize {
    harness.app().windows()[0].layout().borrow().keys().len()
}

/// Everything the window's document says, as one string.
pub(crate) fn text(harness: &Harness<Runtime>) -> String {
    let window = &harness.app().windows()[0];
    window.dom().text_content(window.dom().root_node())
}
