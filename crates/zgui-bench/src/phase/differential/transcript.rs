//! `verify`: the picture each window shows, as the drawing of its last frame, live against a
//! window that records everything again from nothing on every turn.
//!
//! Two windows, the same script, and a line-by-line comparison of what each shows. It is the
//! standing gate `cargo xtask ci` runs, and no phase may relax it.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::draw::mounted_recorder;
use crate::gallery::{HEIGHT, WIDTH, mounted_scheme};
use crate::input::focus_a_field;
use crate::inspect::{document, swatch_centres};

use crate::phase::Driver;
use crate::script::{Driven, run_step, script};

/// A set of steps as one word, so a run can be held to the steps it named rather than to a count.
fn list(steps: &[usize]) -> String {
    if steps.is_empty() {
        return "none".to_owned();
    }
    steps
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

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
        ledger: _,
        ticks: _,
    } = driver;
    let (_middle, _away) = (*middle, *away);
    let (boxes, fragments, _repeats) = (*boxes, *fragments, *repeats);
    let (live_scheme, live_full) = (*scheme, std::rc::Rc::clone(full));
    let size = size.clone();
    let _ = (boxes, fragments, live_scheme, &live_full, &size);
    Some(match phase {
        "verify" => {
            let mut cold = crate::gallery::harness(&size);
            let cold_scheme = mounted_scheme();
            let cold_full = mounted_recorder();
            cold.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(WIDTH),
                DevicePx(HEIGHT),
            )));
            crate::verify::settle_cold(&mut cold, 256);
            let cold_centres = swatch_centres(&cold.app().windows()[0]);
            assert_eq!(&cold_centres, &*centres, "the two windows opened the same");
            let live_window = Driven {
                cold: false,
                centres: centres.clone(),
                scheme: live_scheme,
            };
            let cold_window = Driven {
                cold: true,
                centres: cold_centres,
                scheme: cold_scheme,
            };
            let tabs = focus_a_field(harness);
            assert_eq!(
                tabs,
                focus_a_field(&mut cold),
                "both windows reached a field in the same number of tabs"
            );
            let mut faulted: Vec<usize> = Vec::new();
            let mut rounded: Vec<usize> = Vec::new();
            let mut checked = 0;
            // What the comparison below is allowed to be believed about. See
            // [`gradient::Coverage`](crate::verify::gradient::Coverage).
            let mut gradients = crate::verify::gradient::Coverage::default();
            let mut glyphs = 0;
            let mut compare_here =
                |step: usize,
                 what: &str,
                 harness: &mut Harness<Runtime>,
                 cold: &mut Harness<Runtime>| {
                    crate::verify::settled(harness, false);
                    crate::verify::settled(cold, true);
                    let (live_drawing, cold_drawing) = (
                        live_full.drawing().expect("the live window drew a frame"),
                        cold_full.drawing().expect("the cold window drew a frame"),
                    );
                    // The control for everything below. Two drawings that agree about nothing
                    // agree, so both windows have to show something.
                    let drawn = live_drawing.instances().len();
                    for (which, drawing) in [("live", &live_drawing), ("cold", &cold_drawing)] {
                        assert!(
                            !drawing.instances().is_empty(),
                            "step {step} ({what}): the {which} window shows nothing, so nothing \
                             here is being compared"
                        );
                    }
                    glyphs += live_drawing.glyphs().len();
                    let (live_list, cold_list) =
                        (live_drawing.to_string(), cold_drawing.to_string());
                    checked += 1;
                    gradients.sample(&live_list);
                    match crate::verify::compare(
                        step,
                        what,
                        &harness.app().windows()[0],
                        &cold.app().windows()[0],
                        (&live_list, &cold_list),
                    ) {
                        // A rounding is reported as loudly as a fault and counted apart from one.
                        // Two engines that reach one position by different routes disagree in the
                        // last bits of a float and always will; a gate that calls that a stale
                        // pixel is a gate nobody can act on, and one that hides it is a gate that
                        // would not notice the day it stopped being the last bits.
                        Some(found) if found.is_rounding() => {
                            rounded.push(step);
                            println!("  VERIFY NEAR  {found}");
                        }
                        Some(found) => {
                            faulted.push(step);
                            println!("  VERIFY FAULT {found}");
                        }
                        None => println!("  VERIFY ok   step {step} ({what}), {drawn} instances"),
                    }
                };
            compare_here(0, "settled", harness, &mut cold);
            for (index, step) in script().iter().enumerate() {
                run_step(harness, step, &live_window);
                run_step(&mut cold, step, &cold_window);
                compare_here(index + 1, &format!("{step:?}"), harness, &mut cold);
            }
            let (live_boxes, live_frags) = document(&harness.app().windows()[0]);
            let transcript = live_full
                .drawing()
                .expect("the live window drew a frame")
                .to_string();
            // Both sets are printed as the steps themselves and not as counts. A count is a number
            // that can be met two ways; the standing gate over this phase holds the run to the
            // steps it named, so a fault that moves from one step to another is a new fault.
            println!(
                "VERIFY size={size} boxes={live_boxes} fragments={live_frags} \
                 steps_checked={checked} faults={} rounded={} transcript_lines={}",
                list(&faulted),
                list(&rounded),
                transcript.lines().count()
            );
            println!(
                "  a rounding is a disagreement in which every difference is a number that moved \
                 by less than {} of a device pixel",
                format_args!("1/{}", (1.0 / crate::verify::GRID).round() as u32),
            );
            println!(
                "  VERIFY covers {} and {glyphs} glyph instances",
                gradients.describe()
            );
            gradients.assert_non_vacuous(&size);
            if std::env::var_os("VERIFY_DUMP").is_some() {
                println!("{transcript}");
            }
            if !faulted.is_empty() {
                eprintln!(
                    "VERIFY failed: the two windows drew different pictures at {} of {checked} \
                     steps: {}",
                    faulted.len(),
                    list(&faulted),
                );
                // Left rather than unwound. The document is mounted in thread-local state that
                // panics when it is dropped out of order, and an abort on the way out of a failure
                // buries the verdict this run exists to state under a second, unrelated one.
                std::process::exit(1);
            }
            0
        }
        // The pixels themselves. After every step the window is asked what it presented, then
        // told it was occluded and un-occluded — which takes full damage and reconfigures the
        // target without touching a single thing about the document — and asked again. The two
        // readbacks are the same state drawn incrementally and drawn whole.
        _ => return None,
    })
}
