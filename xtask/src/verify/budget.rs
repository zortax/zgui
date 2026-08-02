//! Every step of the script at which the two windows are not identical, named one at a time.
//!
//! # Why this is a list and not a number
//!
//! A count is a claim two different worlds satisfy: the day one fault is fixed and another appears,
//! a budget of nine is still met and nobody hears about either. A list of steps is a claim about the
//! world — it says *which* comparisons are known not to agree, so a fault that moves is a fault that
//! fails the build, and a fault that is fixed fails it too until the entry is removed.
//!
//! Each entry below is something a person can go and check. The step number is the step of the
//! script the run prints beside it; [`Known::because`] is what was found when it was looked at.
//!
//! # Why the count is the same at every size, and what is allowed to move it
//!
//! [`Known::steps`] is the length of the script plus the settled comparison taken before it, so it
//! is a property of the script and not of the document, and every size carries the same number. It
//! moves when the script does, and the only safe way to move the script is to **append**: a part
//! added at the end leaves every step number in the lists below meaning what it meant, while a part
//! inserted anywhere else silently renumbers every fault after it into a different step.
//!
//! It reads 108 rather than 95 because the script gained a part that scrolls a port smaller than
//! the window — thirteen steps, all at the end.

/// Where this list lives, for a failure that says what to edit.
pub(crate) const HERE: &str = "xtask/src/verify/budget.rs";

/// What one document size is known to disagree about.
pub(crate) struct Known {
    /// The gallery size, as the harness names it.
    pub(crate) size: &'static str,
    /// How many comparisons the run must take, so a run that stopped comparing cannot pass.
    pub(crate) steps: usize,
    /// The steps at which the two windows draw different pictures.
    pub(crate) faults: &'static [usize],
    /// The steps at which they differ only in the last bits of a float.
    ///
    /// Pinned as exactly as the faults are. A rounding is not a defect, but a *new* one is a new
    /// place where two routes to one position stopped agreeing, and the day one of these becomes a
    /// whole pixel is the day it matters that somebody was watching.
    pub(crate) roundings: &'static [usize],
    /// The steps at which a disagreement is accepted whether or not it happens.
    ///
    /// The one thing a list of steps cannot express, and it is here because pretending otherwise
    /// would make the whole list unreliable rather than this part of it. A step here is a step that
    /// disagrees in *some* runs of one binary — so neither "it faulted" nor "it did not" can be
    /// asserted, and the honest thing is to say which steps those are, what the disagreement is
    /// when it happens, and how often. Every other step of that size is still held exactly.
    ///
    /// An entry here is a defect that is open, not a difference that is accepted. See
    /// [`Known::unstable_because`].
    pub(crate) unstable: &'static [usize],
    /// What the faults are, in one sentence, for the run to print and a reader to disagree with.
    pub(crate) because: &'static str,
    /// What the unstable steps do when they do it, and what is known about why.
    pub(crate) unstable_because: &'static str,
}

/// Nothing is known to disagree, which is what most of the documents say.
const AGREED: &str = "no step of the script draws a different picture in the two windows";

/// The colour-scheme flip that does not always land, and everything measured about it.
///
/// **An open defect, recorded here because it is intermittent and therefore cannot be pinned as a
/// fault or as agreement.** At `s1`, and only at `s1`, a `Theme` step sometimes leaves one element's
/// border drawn in the *other* scheme's token — 0.2118, 0.2275, 0.2471 against 0.851, 0.851, 0.8784
/// on an 82x36 box — while every other one of the three hundred lines of the display list agrees.
/// It is one element and one paint, so it is not the scheme failing to reach the document; it is one
/// element keeping the colour it had.
///
/// Measured: **4 runs of 40** disagreed, at one of the `Theme` steps (37, 39 and 70 each seen). The
/// disagreement then survives every following step until something repaints that element — one run
/// carried it from step 39 to step 61 — which is why the entry is a span rather than three steps,
/// and why an earlier reading of this gate saw "0, 2 and 20 faults across runs of one binary".
///
/// What it is not: address-space layout (20 runs with randomisation disabled flake at the same
/// rate), a worker race (the window restyles with no pool), or the wall clock (the harness runs on a
/// virtual one and nothing in the frame path reads a real instant to decide what to do). What it is
/// remains open.
const A_THEME_FLIP_THAT_DOES_NOT_ALWAYS_LAND: &str = "steps 37-61 and 70-73: a Theme step sometimes leaves one element's border in the other \
     scheme's token, in about one run in ten, and the difference then survives until that element \
     is repainted. One element, one paint, every other line of the display list identical. An open \
     defect: see the module comment in xtask/src/verify/budget.rs for everything measured about it";

/// Nothing about this size is intermittent, which is what five of the seven documents say.
const NOTHING_UNSTABLE: &str = "nothing here is intermittent";

/// The picked swatch that does not always have its border yet, and everything measured about it.
///
/// **An open defect, recorded here because it is intermittent and therefore cannot be pinned as a
/// fault or as agreement.** At `s2`, and only at `s2`, the `Click(1)` at step 3 sometimes leaves the
/// two windows disagreeing about exactly one quad: the swatch the click just picked. The window that
/// has been running draws its 2px `#7ee3ff` border; the one rebuilt from nothing draws
/// `stroke=none`. Same box, same fill, same radii, same clip — one paint on one element, and all
/// thirty-seven other lines of the transcript identical.
///
/// The element is the harness's own probe row, not a component: `.swatch` in
/// `crates/zgui-bench/src/gallery/probe.rs`, whose selected state is a `class:` toggle driven by a
/// signal the click sets. So what disagrees is when the class reaches the display list relative to
/// the frame each window captures, in two windows that run different numbers of frames by
/// construction — not anything about the library being drawn around it.
///
/// Measured: **7 runs of 37** disagreed, every one of them at step 3 and at no other step, and
/// every one of them on that single line. It does not spread: the following steps agree in every
/// run, so unlike the `s1` flip this one is repainted immediately rather than carried forwards. The
/// rate is not steady — one sample of twelve gave six and a later sample of twenty-five gave one —
/// so it is sensitive to something outside the run, which is a further thing known about it and not
/// a reason to call it either rate.
const A_PICKED_SWATCH_THAT_IS_NOT_ALWAYS_BORDERED_YET: &str = "step 3: the Click(1) sometimes leaves the picked swatch's 2px border drawn in the running \
     window and not in the rebuilt one, in about one run in five. One element, one paint, every \
     other line of the display list identical, and the next step agrees again. The element is the \
     harness's own probe swatch and its selected state is a class toggle. An open defect: see the \
     module comment in xtask/src/verify/budget.rs for everything measured about it";

/// What every document size is known to disagree about, and where it agrees.
pub(crate) const KNOWN: &[Known] = &[
    Known {
        size: "s0",
        steps: 108,
        faults: &[],
        roundings: &[],
        unstable: &[],
        because: AGREED,
        unstable_because: NOTHING_UNSTABLE,
    },
    Known {
        size: "s1",
        steps: 108,
        faults: &[],
        roundings: &[],
        // The span the colour-scheme flip can reach, which is from the first `Theme` step to the
        // step that repaints the element it leaves behind, and again from the second. Every step of
        // `s1` outside it is held exactly.
        unstable: &[
            37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58,
            59, 60, 61, 70, 71, 72, 73,
        ],
        because: AGREED,
        unstable_because: A_THEME_FLIP_THAT_DOES_NOT_ALWAYS_LAND,
    },
    Known {
        size: "s2",
        steps: 108,
        faults: &[],
        // Nothing rounds here any more. Every step of this document used to land the two windows a
        // fraction of a device pixel apart at nineteen of the ninety-five; they now agree exactly.
        roundings: &[],
        // The one step whose disagreement cannot be asserted either way. See
        // [`A_PICKED_SWATCH_THAT_IS_NOT_ALWAYS_BORDERED_YET`]; every other step is held exactly.
        unstable: &[3],
        because: AGREED,
        unstable_because: A_PICKED_SWATCH_THAT_IS_NOT_ALWAYS_BORDERED_YET,
    },
    Known {
        size: "s4",
        steps: 108,
        faults: &[],
        roundings: &[],
        unstable: &[],
        because: AGREED,
        unstable_because: NOTHING_UNSTABLE,
    },
    Known {
        size: "s8",
        steps: 108,
        faults: &[],
        roundings: &[],
        unstable: &[],
        because: AGREED,
        unstable_because: NOTHING_UNSTABLE,
    },
    Known {
        size: "s13",
        steps: 108,
        // The fling-into-a-ratio-change is gone. Step 61 used to leave the two windows' scroll
        // offsets 0.43 device pixels apart inside the elastic displacement, which `Scale(1.5)` then
        // multiplied into a whole pixel across the whole document for nine steps. Both windows now
        // come to rest at the same place: five runs, no fault at any step.
        faults: &[],
        roundings: &[],
        unstable: &[],
        because: AGREED,
        unstable_because: NOTHING_UNSTABLE,
    },
    Known {
        size: "s26",
        steps: 108,
        faults: &[],
        roundings: &[],
        unstable: &[],
        because: AGREED,
        unstable_because: NOTHING_UNSTABLE,
    },
];

#[cfg(test)]
mod tests {
    use super::{AGREED, KNOWN};

    #[test]
    fn every_size_the_harness_ships_is_budgeted() {
        // The list of documents lives in the harness; a size added there and not here would be a
        // size this gate silently stopped running.
        let sizes = ["s0", "s1", "s2", "s4", "s8", "s13", "s26"];
        let held: Vec<&str> = KNOWN.iter().map(|known| known.size).collect();
        assert_eq!(held, sizes, "every document size is budgeted, in order");
    }

    #[test]
    fn a_size_with_faults_says_what_they_are_and_one_without_says_it_has_none() {
        for known in KNOWN {
            if known.faults.is_empty() {
                assert_eq!(
                    known.because, AGREED,
                    "{} has no faults and must not claim a reason for any",
                    known.size
                );
            } else {
                assert!(
                    known.because.len() > 80
                        && known.because.contains(&known.faults[0].to_string()),
                    "{} budgets faults without naming them: {}",
                    known.size,
                    known.because
                );
            }
        }
    }

    #[test]
    fn no_step_is_both_a_fault_and_a_rounding() {
        for known in KNOWN {
            for step in known.faults {
                assert!(
                    !known.roundings.contains(step),
                    "{} step {step} is budgeted twice",
                    known.size
                );
            }
        }
    }

    #[test]
    fn every_budgeted_step_is_a_step_the_run_takes() {
        for known in KNOWN {
            for step in known.faults.iter().chain(known.roundings) {
                assert!(
                    *step < known.steps,
                    "{} budgets step {step} and only takes {}",
                    known.size,
                    known.steps
                );
            }
        }
    }
}
