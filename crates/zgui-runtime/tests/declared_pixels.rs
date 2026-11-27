//! A window's declared damage holds every pixel its commits change.
//!
//! With verification off, the compositor takes a transaction's declared damage in place of what it
//! derives, so a pixel a commit changes outside the declaration keeps what the frame before drew.
//! Each step of an editing script is therefore drawn incrementally on this machine's device and
//! compared with the same picture drawn again from nothing.

mod support;

use std::sync::Arc;

use iceblink::{RenderTarget, Renderer};
use zgui_geom::{CssPx, DevicePx, Point, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_runtime::{App, AppError};
use zgui_testkit_compositor::{Composed, Device, Pixels};
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{
    Key, KeyCode, KeyEvent, KeyLocation, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
    PointerEvent, Timestamp,
};

/// The sheet the field is styled by, with its text off the surface's whole pixels at the scale
/// the window is shown at.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 11.3px 19.7px }
                   editor { display: block; width: 200px; height: 40px; color: rgb(0, 102, 204) }
                   text { display: block; color: rgb(0, 102, 204) }";

/// The scale the window is shown at, which puts glyph edges between pixels.
const SCALE: f64 = 1.5;

/// A window holding a focused editor, drawn on this machine's device.
struct Field {
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    composed: Composed,
}

impl Field {
    /// Opens the window, or returns `None` when no device opens.
    fn open(content: &'static str) -> Option<Self> {
        if let Err(why) = Device::open([64, 64]) {
            eprintln!("skipped: {why}");
            return None;
        }
        let composed = Composed::default();
        let into = composed.clone();
        let mut compositor = support::compositor();
        // The declaration is taken in place of the derived damage only without verification.
        compositor.set_verification(false);
        let editor = NodeRef::new();
        let handler = App::new()
            .with_title("declared")
            .with_size(400.0, 300.0)
            .with_stylesheet(CSS)
            .with_compositor(support::driver(compositor))
            .with_renderer(Box::new(
                move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                    let into = into.clone();
                    Ok::<_, AppError>(zgui_testkit_compositor::factory(move || {
                        let mut device = Device::open(target.size)
                            .expect("a device opens, which the test answered beforehand")
                            .composing_into(into);
                        device.configure(target);
                        device
                    }))
                },
            ))
            .with_text_engine(Box::new(|| {
                Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
            }))
            .into_handler(move |cx: &mut BuildCx<'_>| {
                Box::new(
                    zgui_elements::column()
                        .class("root")
                        .child(zgui_elements::editor().node_ref(editor).child(content))
                        .into_view()
                        .build(cx),
                )
            })
            .expect("the reactive runtime installs");
        let mut field = Self {
            harness: zgui_platform_headless::Harness::new(handler),
            composed,
        };
        field
            .harness
            .deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: SCALE,
                size: Size::new(
                    DevicePx((400.0 * SCALE) as f32),
                    DevicePx((300.0 * SCALE) as f32),
                ),
            });
        field.harness.settle(16);
        field.press(KeyEvent::named(
            NamedKey::Tab,
            PhysicalKey::Code(KeyCode::Tab),
        ));
        Some(field)
    }

    /// Delivers one key press and lets its frames settle.
    fn press_with(&mut self, event: KeyEvent, modifiers: Modifiers) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event,
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(16);
    }

    /// Delivers one key press with no modifier.
    fn press(&mut self, event: KeyEvent) {
        self.press_with(event, Modifiers::NONE);
    }

    /// Types `text`, one character at a time.
    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.press(KeyEvent::character(character.to_string()));
        }
    }

    /// Selects everything in the field.
    fn select_all(&mut self) {
        let event = KeyEvent {
            key: Key::Character("a".into()),
            key_without_modifiers: Key::Character("a".into()),
            physical: PhysicalKey::Code(KeyCode::KeyA),
            location: KeyLocation::Standard,
            repeat: false,
        };
        self.press_with(event, Modifiers::PRIMARY);
    }

    /// Clicks at `x`, `y` in CSS pixels.
    fn click(&mut self, x: f32, y: f32) {
        for action in [PointerAction::Pressed, PointerAction::Released] {
            self.harness.deliver_to_first(SurfaceEvent::Pointer {
                action,
                event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
        self.harness.settle(16);
    }

    /// The picture the device shows now.
    fn shown(&self) -> Arc<Pixels> {
        self.composed.pixels().expect("the window drew a frame")
    }

    /// The same picture drawn again from nothing.
    fn redrawn(&mut self) -> Arc<Pixels> {
        let window = &self.harness.app().windows()[0];
        window.forget_compositor_sync();
        window.request_frame();
        self.harness.settle(16);
        self.shown()
    }

    /// Panics, naming `step`, when the incremental picture differs from the redrawn one.
    fn assert_whole(&mut self, step: &str) {
        let shown = self.shown();
        let redrawn = self.redrawn();
        let size = shown.size();
        assert_eq!(
            size,
            redrawn.size(),
            "{step}: the two pictures differ in size"
        );
        let mut stale = Vec::new();
        for y in 0..size.height {
            for x in 0..size.width {
                if shown.rgba(x, y) != redrawn.rgba(x, y) {
                    stale.push((x, y));
                }
            }
        }
        assert!(
            stale.is_empty(),
            "{step}: {} pixels differ from a redraw, first {:?}",
            stale.len(),
            &stale[..stale.len().min(8)]
        );
    }
}

#[test]
fn every_pixel_an_edit_changes_is_drawn_without_verification() {
    let Some(mut field) = Field::open("wide words") else {
        return;
    };
    field.assert_whole("focus");
    field.type_text("abc");
    field.assert_whole("typing");
    field.select_all();
    field.assert_whole("select all");
    field.type_text("x");
    field.assert_whole("replacing the selection");
    field.press(KeyEvent::named(
        NamedKey::Backspace,
        PhysicalKey::Code(KeyCode::Backspace),
    ));
    field.assert_whole("deleting");
    field.type_text("wider words than before");
    field.click(40.0, 30.0);
    field.assert_whole("a click into the text");
    field.select_all();
    field.type_text("w");
    field.assert_whole("a shorter text");
}
