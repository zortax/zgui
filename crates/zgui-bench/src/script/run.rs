//! Delivering one step to one window.
//!
//! Two windows are driven through the same sequence, and one of them throws every held result away
//! before each turn of the loop. That is the whole of the differential: identical input, one engine
//! answering incrementally and one answering from nothing, and any disagreement between what they
//! draw is a saving that was not sound.

use zgui::geom::{CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::runtime::Runtime;
use zgui::vocab::{KeyCode, KeyEvent, NamedKey, PhysicalKey, PointerAction, ScrollPhase};
use zgui_platform_headless::Harness;
use zgui_ui_tokens::prelude::*;

use crate::gallery::{HEIGHT, WIDTH};
use crate::input::wheel;
use crate::input::{drag, gliding, key, key_up, pointer};
use crate::inspect::testid_centre;
use crate::script::Driven;
use crate::script::step::Step;

/// Drives one step, either incrementally or with every held result thrown away first.
pub(crate) fn run_step(harness: &mut Harness<Runtime>, step: &Step, window: &Driven) {
    let (cold, centres) = (window.cold, &window.centres);
    let middle = Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0));
    let settle = |harness: &mut Harness<Runtime>| {
        if cold {
            crate::verify::settle_cold(harness, 96);
        } else {
            harness.settle(96);
        }
    };
    match step {
        Step::Hover(index) => {
            harness.deliver_to_first(pointer(PointerAction::Moved, centres[*index]));
            settle(harness);
        }
        Step::Click(index) => {
            harness.deliver_to_first(pointer(PointerAction::Moved, centres[*index]));
            settle(harness);
            harness.deliver_to_first(pointer(PointerAction::Pressed, centres[*index]));
            settle(harness);
            harness.deliver_to_first(pointer(PointerAction::Released, centres[*index]));
            settle(harness);
        }
        Step::Notch(lines) => {
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            settle(harness);
            harness.deliver_to_first(wheel(middle, *lines));
            settle(harness);
            let mut carried = 0;
            while gliding(harness) && carried < 90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                carried += 1;
            }
        }
        Step::Inside(lines) => {
            let Some(at) = testid_centre(&harness.app().windows()[0], "probe-port") else {
                println!("  step skipped: nothing carries data-testid=probe-port");
                return;
            };
            harness.deliver_to_first(pointer(PointerAction::Moved, at));
            settle(harness);
            harness.deliver_to_first(wheel(at, *lines));
            settle(harness);
            let mut carried = 0;
            while gliding(harness) && carried < 90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                carried += 1;
            }
        }
        Step::Type(letter) => {
            harness.deliver_to_first(key(KeyEvent::character(*letter)));
            settle(harness);
            harness.deliver_to_first(key_up(KeyEvent::character(*letter)));
            settle(harness);
        }
        Step::Rub => {
            let event =
                || KeyEvent::named(NamedKey::Backspace, PhysicalKey::Code(KeyCode::Backspace));
            harness.deliver_to_first(key(event()));
            settle(harness);
            harness.deliver_to_first(key_up(event()));
            settle(harness);
        }
        Step::Resize(width) => {
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(*width),
                DevicePx(HEIGHT),
            )));
            settle(harness);
            // The pace gate refuses a second configure inside one refresh interval, so a resize
            // that is not followed over that boundary leaves a frame owed and the two windows
            // compared in the middle of one.
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::Sized(width, height) => {
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(*width),
                DevicePx(*height),
            )));
            settle(harness);
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::Scale(scale) => {
            harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: f64::from(*scale),
                size: Size::new(DevicePx(WIDTH * scale), DevicePx(HEIGHT * scale)),
            });
            settle(harness);
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::GlideResize {
            lines,
            after,
            width,
            height,
        } => {
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            settle(harness);
            harness.deliver_to_first(wheel(middle, *lines));
            settle(harness);
            let mut carried = 0;
            while gliding(harness) && carried < *after {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                carried += 1;
            }
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(*width),
                DevicePx(*height),
            )));
            settle(harness);
            let mut left = 0;
            while gliding(harness) && left < 90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                left += 1;
            }
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::EdgeDrag { from, to, steps } => {
            for index in 1..=*steps {
                let at = from + (to - from) * (index as f32 / *steps as f32);
                harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                    DevicePx(at.round()),
                    DevicePx(HEIGHT),
                )));
                settle(harness);
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
            }
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::Wait => {
            harness.advance(std::time::Duration::from_micros(16_667));
            if cold {
                crate::verify::forget(harness);
            }
            harness.pump();
        }
        Step::Drag(pixels) => {
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            settle(harness);
            let each = pixels / 8.0;
            for index in 0..8 {
                let phase = if index == 0 {
                    ScrollPhase::Started
                } else {
                    ScrollPhase::Moved
                };
                harness.deliver_to_first(drag(middle, each, phase));
                settle(harness);
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
            }
            harness.deliver_to_first(drag(middle, 0.0, ScrollPhase::Ended));
            settle(harness);
            let mut carried = 0;
            while gliding(harness) && carried < 90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                carried += 1;
            }
        }
        // A held gesture pushed past an edge, released, and a configure delivered into the return.
        //
        // The push is delivered as a held drag rather than as a notch because only a held gesture
        // produces an elastic displacement at all: a wheel notch past the end of a document is
        // clamped and there is nothing to spring back from.
        Step::Spring {
            pixels,
            after,
            width,
            height,
        } => {
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            settle(harness);
            let each = pixels / 8.0;
            for index in 0..8 {
                let phase = if index == 0 {
                    ScrollPhase::Started
                } else {
                    ScrollPhase::Moved
                };
                harness.deliver_to_first(drag(middle, each, phase));
                settle(harness);
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
            }
            harness.deliver_to_first(drag(middle, 0.0, ScrollPhase::Ended));
            settle(harness);
            let mut carried = 0;
            while gliding(harness) && carried < *after {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                carried += 1;
            }
            // What the configure is about to land in the middle of. A step that reports a settled
            // scroller pushed at an edge that was not there — the container was not at its end, or
            // the document at this size does not scroll at all — and is comparing a plain resize.
            // It is printed rather than asserted because at the smallest sizes there is genuinely
            // nothing to overscroll, and a size where a step is vacuous is a fact about the size.
            if !cold {
                let stretched = !harness.app().windows()[0].scroll().borrow().settled();
                println!("  spring: stretched={stretched} after {carried} frames of return");
            }
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(*width),
                DevicePx(*height),
            )));
            settle(harness);
            let mut left = 0;
            while gliding(harness) && left < 90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
                left += 1;
            }
            // The pace gate owes a frame for the configure exactly as it does after any other
            // resize, and a comparison taken before it is paid is taken in the middle of one.
            harness.advance(std::time::Duration::from_micros(16_667));
            settle(harness);
        }
        Step::Fling(lines) => run_step(harness, &Step::Notch(*lines), window),
        Step::Press(name) => {
            let Some(at) = testid_centre(&harness.app().windows()[0], name) else {
                println!("  step skipped: nothing carries data-testid={name}");
                return;
            };
            for action in [
                PointerAction::Moved,
                PointerAction::Pressed,
                PointerAction::Released,
            ] {
                harness.deliver_to_first(pointer(action, at));
                settle(harness);
            }
            // An overlay opens with a transition, and a comparison taken in the middle of one is
            // comparing two documents that are entitled to be at different points of it.
            for _ in 0..30 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
            }
        }
        Step::Dismiss => {
            let event = || KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
            harness.deliver_to_first(key(event()));
            settle(harness);
            harness.deliver_to_first(key_up(event()));
            settle(harness);
            for _ in 0..30 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                harness.pump();
            }
        }
        Step::Theme => {
            let scheme = window.scheme;
            scheme.set(match scheme.get_untracked() {
                ColorScheme::Dark => ColorScheme::Light,
                _ => ColorScheme::Dark,
            });
            settle(harness);
            for _ in 0..30 {
                harness.advance(std::time::Duration::from_micros(16_667));
                if cold {
                    crate::verify::forget(harness);
                }
                let ran = harness.pump();
                if std::env::var_os("THEME_TRACE").is_some() {
                    eprintln!(
                        "  TRACE cold={cold} ran={ran} prims={}",
                        harness.app().windows()[0].scene().primitives.len()
                    );
                }
            }
        }
    }
}
