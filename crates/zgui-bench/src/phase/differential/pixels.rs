//! `pixels`: what was actually drawn.
//!
//! The picture differential. Two windows again, but the comparison is of rasterised output rather
//! than of the list that produced it — which is the only instrument that sees a fault the list
//! describes correctly and the renderer draws wrongly. It needs a device, so it is a probe rather
//! than a step of `ci`.

use crate::inspect::{Painted, scroll_signature};

use crate::draw::CAPTURE;
use crate::phase::Driver;
use crate::script::{Driven, run_step, script};

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
        "pixels" => {
            let live_window = Driven {
                cold: false,
                centres: centres.clone(),
                scheme: live_scheme,
            };
            let capture = CAPTURE.with(std::clone::Clone::clone);
            let mut faults = 0;
            let mut checked = 0;
            // Every frame is read back for the whole phase. A step that draws nothing leaves the
            // previous readback standing, which is exactly right: nothing was drawn, so what is on
            // the screen is what was on it.
            capture.want.set(true);
            capture
                .transcribe
                .set(std::env::var_os("PIXELS_DUMP").is_some());
            // One full repaint before the first step, so that "what is on the screen" is a picture
            // rather than nothing. A step that draws no frame leaves the last readback standing,
            // which is right — but at the smallest document sizes the first step of the script
            // draws nothing at all, and without a picture behind it there is nothing to leave.
            crate::verify::repaint_everything(harness, false);
            let shoot = || {
                (
                    capture
                        .last
                        .borrow()
                        .clone()
                        .expect("the device has drawn at least one frame"),
                    capture
                        .composed
                        .borrow()
                        .clone()
                        .expect("the device has composed at least one frame"),
                )
            };
            let mut painted = Painted::default();
            for (index, step) in script().iter().enumerate() {
                capture.damages.borrow_mut().clear();
                capture.full_lists.borrow_mut().clear();
                capture.every_list.borrow_mut().clear();
                run_step(harness, step, &live_window);
                let drawn = std::mem::take(&mut *capture.damages.borrow_mut());
                let full_lists = std::mem::take(&mut *capture.full_lists.borrow_mut());
                let every_list = std::mem::take(&mut *capture.every_list.borrow_mut());
                let (incremental, incremental_composed) = shoot();
                let scrolled_before = scroll_signature(&harness.app().windows()[0]);
                // Where every fragment was when the step ended. The control repaint is meant to
                // change no input to any pixel, and a scroll position that held still is not enough
                // to establish that: a box laid out again between the two pictures moves everything
                // below it while the offset stays exactly where it was.
                let placed_before = crate::verify::geometry(&harness.app().windows()[0]);
                capture.full_lists.borrow_mut().clear();
                crate::verify::repaint_everything(harness, false);
                let (whole, whole_composed) = shoot();
                let scrolled_after = scroll_signature(&harness.app().windows()[0]);
                let placed_after = crate::verify::geometry(&harness.app().windows()[0]);
                let whole_lists = std::mem::take(&mut *capture.full_lists.borrow_mut());
                for list in &whole_lists {
                    painted.saw(list);
                }
                // A repaint that answers every pixel and emits no primitive is the one fault two
                // pictures cannot show: the window keeps whatever it had, so the comparison agrees
                // while the frame a viewer arriving now would be given is empty. It is what a
                // window opening under a desktop that is already dark would look like, so it is
                // asked after every step rather than only after the ones that change a colour.
                let repainted = whole_lists
                    .iter()
                    .map(|list| list.lines().filter(|line| line.starts_with("  ")).count())
                    .max()
                    .unwrap_or(0);
                if repainted == 0 {
                    faults += 1;
                    println!(
                        "  PIXELS BLANK step {} ({step:?}): {} frames answered full damage and the \
                         most any of them emitted was no primitive at all",
                        index + 1,
                        whole_lists.len()
                    );
                }
                // The control has to be stable before a difference against it means anything: two
                // full repaints of one unchanged state must be the same picture.
                //
                // Where they are not, the difference between them is this step's noise floor and it
                // is reported. A repaint is a paced frame, so taking one costs a little of the
                // clock, and a document with a shimmer or a spinner in it is entitled to have moved
                // by that much between two of them — a moving gradient a few milliseconds apart
                // differs by a channel step or two over the whole band it covers. What that cannot
                // do is exceed the same instrument's own disagreement with itself, so that is what a
                // difference has to beat to be a fault.
                crate::verify::repaint_everything(harness, false);
                let (again, _) = shoot();
                let noise = again.max_difference(&whole);
                if again != whole {
                    println!(
                        "  PIXELS CONTROL UNSTABLE step {}: two full repaints of one unchanged \
                         state differ by {noise}",
                        index + 1,
                    );
                }
                checked += 1;
                let worst = incremental.max_difference(&whole);
                let composed_worst = incremental_composed.max_difference(&whole_composed);
                if composed_worst != worst {
                    println!(
                        "  PIXELS note step {}: composed differs by {composed_worst}, presented by \
                         {worst}",
                        index + 1
                    );
                }
                if worst <= noise {
                    println!(
                        "  PIXELS ok   step {} ({step:?}): {worst} against a noise floor of {noise}",
                        index + 1
                    );
                } else {
                    faults += 1;
                    let size = incremental.size();
                    let (mut count, mut bounds) = (0_u64, None::<(i32, i32, i32, i32)>);
                    for y in 0..size.height {
                        for x in 0..size.width {
                            if incremental.rgba(x, y) != whole.rgba(x, y) {
                                count += 1;
                                bounds = Some(match bounds {
                                    None => (x, y, x, y),
                                    Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                                });
                            }
                        }
                    }
                    let sample = bounds.map(|(l, t, _, _)| {
                        format!(
                            "at ({l},{t}) live {:?} whole {:?}",
                            incremental.rgba(l, t),
                            whole.rgba(l, t)
                        )
                    });
                    println!(
                        "  PIXELS FAULT step {} ({step:?}): {count} px differ, max channel {worst}, \
                         bbox {bounds:?}, {}",
                        index + 1,
                        sample.as_deref().unwrap_or("-")
                    );
                    // Which of the two things a difference can be. The control repaint takes the
                    // clock with it, so a document that is still moving is a document photographed
                    // twice in two places and the difference is the harness's own. A position that
                    // is the same either side of the repaint rules that out, and what is left is a
                    // frame that composed the document somewhere a repaint of the same state does
                    // not — which is a fault in the engine and not in the picture.
                    println!(
                        "    scroll: {} before the control repaint, {} after — {}",
                        scrolled_before,
                        scrolled_after,
                        if scrolled_before == scrolled_after {
                            "one state, so the difference is what was drawn"
                        } else {
                            "two states, so the document moved between the pictures"
                        }
                    );
                    let moved = placed_before
                        .iter()
                        .zip(placed_after.iter())
                        .filter(|(one, two)| one != two)
                        .count()
                        + placed_before.len().abs_diff(placed_after.len());
                    println!(
                        "    layout: {moved} of {} fragments moved across the control repaint",
                        placed_before.len()
                    );
                    let mut covered = 0;
                    let mut full = 0;
                    for (whole_surface, rects) in &drawn {
                        if *whole_surface {
                            full += 1;
                        }
                        for rect in rects {
                            if let Some((l, t, r, b)) = bounds
                                && rect.origin.x <= l
                                && rect.origin.y <= t
                                && rect.origin.x + rect.size.width >= r
                                && rect.origin.y + rect.size.height >= b
                            {
                                covered += 1;
                            }
                        }
                    }
                    println!(
                        "    damage: {} frames, {full} of them full, {covered} rects covered the \
                         whole difference",
                        drawn.len()
                    );
                    for (which, list) in full_lists.iter().enumerate() {
                        let quads = list.lines().filter(|line| line.contains("quad ")).count();
                        let sprites = list.lines().filter(|line| line.contains("sprite")).count();
                        println!(
                            "    full-damage frame {which}: {} lines, {quads} quads, {sprites} \
                             sprites",
                            list.lines().count()
                        );
                    }
                    for (which, list) in whole_lists.iter().enumerate() {
                        println!(
                            "    unocclusion repaint {which}: {} lines, {} quads, {} sprites",
                            list.lines().count(),
                            list.lines().filter(|line| line.contains("quad ")).count(),
                            list.lines().filter(|line| line.contains("sprite")).count()
                        );
                    }
                    if let Some(dir) = std::env::var_os("PIXELS_DUMP") {
                        let dir = std::path::PathBuf::from(dir);
                        for (name, held) in [("live", &incremental), ("whole", &whole)] {
                            let mut bytes = Vec::new();
                            for y in 0..size.height {
                                for x in 0..size.width {
                                    bytes.extend_from_slice(&held.rgba(x, y));
                                }
                            }
                            let file = dir.join(format!(
                                "step{:02}-{name}-{}x{}.rgba",
                                index + 1,
                                size.width,
                                size.height
                            ));
                            std::fs::write(file, bytes).expect("the dump is written");
                        }
                        // The pictures say *that* the two frames differ; the lists say what each of
                        // them drew. A displacement is a transform or an origin, and neither is
                        // visible in a readback.
                        for (name, lists) in [("frame", &full_lists), ("repaint", &whole_lists)] {
                            for (which, list) in lists.iter().enumerate() {
                                let file =
                                    dir.join(format!("step{:02}-{name}{which}.txt", index + 1));
                                std::fs::write(file, list).expect("the list is written");
                            }
                        }
                        for (which, (against, list)) in every_list.iter().enumerate() {
                            let file =
                                dir.join(format!("step{:02}-every{which:03}.txt", index + 1));
                            std::fs::write(file, format!("drawn against {against}\n{list}"))
                                .expect("the list is written");
                        }
                    }
                }
            }
            println!("PIXELS size={size} steps_checked={checked} faults={faults} {painted}");
            // A differential that names its faults and exits zero is a differential nobody's gate
            // ever fails on. What it found is what it is for.
            assert_eq!(faults, 0, "the two pictures disagreed {faults} times");

            0
        }
        _ => return None,
    })
}
