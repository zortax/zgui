//! `theme-check`: the display list either side of a colour-scheme flip.
//!
//! The one comparison that needs no second window. A scheme flip changes custom properties and
//! nothing else, so the document before and after is the same document in two palettes — and every
//! difference between the two lists is either a colour or a fault.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_ui_tokens::prelude::*;

use crate::draw::mounted_recorder;
use crate::gallery::{HEIGHT, WIDTH, mounted_scheme, runtime};

use crate::phase::Driver;

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    let Driver {
        harness,
        scheme,
        full,
        centres: _,
        middle,
        away,
        boxes,
        fragments,
        size,
        repeats,
        ledger: _,
        ticks: _,
    } = driver;
    let (_middle, _away) = (*middle, *away);
    let (boxes, fragments, _repeats) = (*boxes, *fragments, *repeats);
    let (live_scheme, live_full) = (*scheme, std::rc::Rc::clone(full));
    let size = size.clone();
    let _ = (boxes, fragments, live_scheme, &live_full, &size);
    Some(match phase {
        "theme-check" => {
            let mut born = Harness::new(runtime(&size));
            let born_scheme = mounted_scheme();
            let born_full = mounted_recorder();
            born_scheme.set(ColorScheme::Dark);
            born.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(WIDTH),
                DevicePx(HEIGHT),
            )));
            born.settle(256);
            born_full.wanted.set(true);
            live_full.wanted.set(true);
            live_scheme.set(ColorScheme::Dark);
            harness.settle(256);
            // A theme flip starts a transition on everything that names a duration, and a
            // comparison taken inside one compares two documents entitled to be at different
            // points of it. Half a second of clock is past every duration the tokens declare.
            for _ in 0..30 {
                harness.advance(std::time::Duration::from_micros(16_667));
                harness.pump();
                born.advance(std::time::Duration::from_micros(16_667));
                born.pump();
            }
            crate::verify::repaint_everything(harness, false);
            crate::verify::repaint_everything(&mut born, false);
            let flipped = live_full
                .taken()
                .expect("the flipped window redrew everything");
            let opened = born_full
                .taken()
                .expect("the born-dark window redrew everything");
            let differing = flipped
                .lines()
                .zip(opened.lines())
                .filter(|(one, two)| one != two)
                .count();
            // Two empty lists agree, and a window that paints nothing is the defect this phase
            // exists to catch — a document built straight into the dark scheme that emits no
            // primitive is a blank window on startup. So both sides have to have drawn something
            // before their agreement means anything.
            for (which, list) in [("flipped", &flipped), ("opened", &opened)] {
                let primitives = list.lines().filter(|line| line.starts_with("  ")).count();
                assert!(
                    primitives > 0,
                    "the {which} window answered full damage with no primitive at all, so nothing \
                     here is being compared"
                );
            }
            println!(
                "THEME-CHECK size={size} flipped_lines={} opened_lines={} differing={differing}",
                flipped.lines().count(),
                opened.lines().count()
            );
            for (index, (one, two)) in flipped.lines().zip(opened.lines()).enumerate() {
                if one != two {
                    println!("  line {index}:\n    flipped {one}\n    opened  {two}");
                    break;
                }
            }
            0
        }
        // The same script driven twice: once by the engine as it ships, once by an engine that
        // is made to recompute everything before every frame. What they put on screen has to be
        // the same thing.
        _ => return None,
    })
}
