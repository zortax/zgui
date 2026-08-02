//! The phases a person can feel: nothing happening, a pointer, a click, a wheel, a keystroke.
//!
//! Each drives one input event and every frame the loop ran before it went quiet, and reports what
//! that cost through the [`Ledger`](crate::stats::Ledger) it was handed. What separates them is not
//! how they are measured but what they ask of the document — an idle turn asks nothing at all, a
//! hover asks for two restyles, a click asks for one, and a wheel asks for a glide.

use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::vocab::{KeyEvent, NamedKey, PhysicalKey, PointerAction};

use crate::input::{gliding, key, key_up, pointer, typing, wheel};

use crate::phase::Driver;

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    let Driver {
        harness,
        scheme,
        full,
        centres,
        middle,
        away,
        boxes,
        fragments,
        size,
        repeats,
        ledger,
        ticks,
    } = driver;
    let (middle, away) = (*middle, *away);
    let (boxes, fragments, repeats) = (*boxes, *fragments, *repeats);
    let (live_scheme, live_full) = (*scheme, std::rc::Rc::clone(full));
    let size = size.clone();
    let _ = (boxes, fragments, live_scheme, &live_full, &size);
    Some(match phase {
        // Nothing touched at all: what the document produces on its own.
        "idle" => {
            let mut frames = 0;
            let start = harness.now();
            for _ in 0..repeats {
                let parked = harness
                    .parked_deadline()
                    .map(|at| at.saturating_duration_since(harness.now()));
                let ran = interaction!(ledger, {
                    harness.advance(std::time::Duration::from_micros(16_700));
                    harness.pump()
                });
                frames += ran;
                println!("  idle turn parked={parked:?} frames={ran}");
            }
            println!(
                "  idle over {:?}: {frames} frames, {} resumes",
                harness.now().saturating_duration_since(start),
                harness.resumes()
            );
            frames
        }
        "hover" => {
            let mut frames = 0;
            for index in 0..repeats {
                let at = centres[index % centres.len()];
                frames += interaction!(ledger, {
                    harness.deliver_to_first(pointer(PointerAction::Moved, at));
                    harness.settle(64)
                });
                frames += interaction!(ticks, {
                    harness.deliver_to_first(pointer(PointerAction::Moved, away));
                    harness.settle(64)
                });
            }
            frames
        }
        "click" => {
            let mut frames = 0;
            for index in 0..repeats {
                let at = centres[index % centres.len()];
                harness.deliver_to_first(pointer(PointerAction::Moved, at));
                frames += harness.settle(64);
                frames += interaction!(ledger, {
                    harness.deliver_to_first(pointer(PointerAction::Pressed, at));
                    harness.settle(64)
                });
                frames += interaction!(ticks, {
                    harness.deliver_to_first(pointer(PointerAction::Released, at));
                    harness.settle(64)
                });
            }
            frames
        }
        "scroll" => {
            let mut frames = 0;
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            frames += harness.settle(64);
            for index in 0..repeats {
                let lines = if (index / 8) % 2 == 0 { 3.0 } else { -3.0 };
                frames += interaction!(ledger, {
                    harness.deliver_to_first(wheel(middle, lines));
                    harness.settle(64)
                });
                frames += interaction!(ticks, {
                    harness.advance(std::time::Duration::from_millis(16));
                    harness.pump()
                });
            }
            frames
        }
        // One notch and the whole glide it starts, tick by tick at the refresh rate.
        "glide" => {
            let mut frames = 0;
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            frames += harness.settle(64);
            for index in 0..repeats {
                let lines = if (index / 4) % 2 == 0 { 3.0 } else { -3.0 };
                frames += interaction!(ledger, {
                    harness.deliver_to_first(wheel(middle, lines));
                    harness.settle(64)
                });
                let mut carried = 0;
                let mut ran = 0;
                while gliding(harness) && carried < 60 {
                    ran += interaction!(ticks, {
                        harness.advance(std::time::Duration::from_micros(16_667));
                        harness.pump()
                    });
                    carried += 1;
                }
                println!("  notch {index}: {carried} glide ticks, {ran} frames");
                frames += ran;
            }
            frames
        }
        // One keystroke into a real text field, found by walking focus until the window tells the
        // surface that text is being typed.
        "keys" => {
            let mut frames = 0;
            harness.deliver_to_first(SurfaceEvent::Focused(true));
            frames += harness.settle(64);
            let mut tabs = 0;
            while !typing(harness) && tabs < 400 {
                harness.deliver_to_first(key(KeyEvent::named(
                    NamedKey::Tab,
                    PhysicalKey::Code(KeyCode::Tab),
                )));
                frames += harness.settle(64);
                harness.deliver_to_first(key_up(KeyEvent::named(
                    NamedKey::Tab,
                    PhysicalKey::Code(KeyCode::Tab),
                )));
                frames += harness.settle(64);
                tabs += 1;
            }
            assert!(typing(harness), "no editable was reached in {tabs} tabs");
            println!("  reached a text field after {tabs} tabs");
            let letters = ["a", "b", "c", "d", "e", "f", "g", "h"];
            for index in 0..repeats {
                let letter = letters[index % letters.len()];
                frames += interaction!(ledger, {
                    harness.deliver_to_first(key(KeyEvent::character(letter)));
                    harness.settle(64)
                });
                harness.deliver_to_first(key_up(KeyEvent::character(letter)));
                frames += harness.settle(64);
                // Back out again every eight, so the field does not grow without bound.
                if index % 8 == 7 {
                    for _ in 0..8 {
                        frames += interaction!(ticks, {
                            harness.deliver_to_first(key(KeyEvent::named(
                                NamedKey::Backspace,
                                PhysicalKey::Code(KeyCode::Backspace),
                            )));
                            harness.settle(64)
                        });
                        harness.deliver_to_first(key_up(KeyEvent::named(
                            NamedKey::Backspace,
                            PhysicalKey::Code(KeyCode::Backspace),
                        )));
                        frames += harness.settle(64);
                    }
                }
            }
            frames
        }
        _ => return None,
    })
}
