//! The standing gate over what a frame *is*: the live window's display list against a rebuild's.
//!
//! The oldest of the compositor programme's gates. Two windows are opened on one document at one
//! size and driven through the same 42-step script event for event; one of them throws every held
//! layout result away before each turn of its loop. After every step both are damaged whole, both
//! redraw everything they hold, and the two finished display lists are compared line for line —
//! along with every fragment's resolved border box, which is the half that sees a subtree drawn
//! somewhere else entirely.
//!
//! # What a green run means, exactly
//!
//! **Every saving the incremental engine took was sound at every step of the script, at seven
//! document sizes.** A frame that declined to lay a box out again, to shape a run again, to emit a
//! primitive again, produced the frame an engine that declined nothing produces — the same
//! primitives, in the same painting order, with the same paints, clips, transforms and geometry.
//!
//! It does not mean the picture is *right*. Both windows draw through one renderer and one layout
//! engine, so an error in either is made twice and cancels; correctness is what the golden
//! transcripts and the component tests decide. What only this reaches is the thing the whole
//! compositor programme risks: a saving that was not sound, which is invisible to every test that
//! runs one window.
//!
//! # Why it runs with a budget rather than at zero
//!
//! It has never been at zero, and until now nothing ran it, so nobody could say what it was not at
//! zero *about*. Now it does: every disagreement is classified by the harness — see the `verify`
//! phase's own documentation for what it takes out of a transcript before comparing and why neither
//! removal is a tolerance — and every one that survives is listed in [`budget`], by size, by step,
//! with what it was found to be. A step that is not on the list fails the build. A step that is on
//! the list and no longer disagrees fails it too, until somebody removes the entry.
//!
//! Three kinds are budgeted apart, because they are not the same claim:
//!
//! * a **fault** is a difference in the picture;
//! * a **rounding** is a disagreement in which every difference is a number that moved by less than
//!   a 256th of a device pixel, which is finer than anything a rasteriser can draw;
//! * an **unstable** step is one that disagrees in some runs of one binary and not others, which
//!   can be asserted in neither direction. One document has such steps and they are an open defect
//!   rather than an accepted difference; every step of every size outside that list is held exactly.
//!
//! In release, and over every document size the harness ships, because the small documents are
//! where a differential is likeliest to be comparing nothing and the large ones are where the
//! coordinate systems are.

mod budget;

use std::path::Path;

use crate::error::{Error, Result};
use crate::process;
use crate::verify::budget::{HERE, KNOWN, Known};

/// The harness that owns the differential.
const HARNESS: &str = "zgui-bench";

/// The binary inside it, named because the harness also ships the reference workloads.
const BINARY: &str = "zgui-bench";

/// The phase, which is also what the gate has always been called.
const PHASE: &str = "verify";

/// Runs the differential at every document size and holds each run to what it is known to disagree
/// about.
///
/// # Errors
///
/// Fails when a run disagrees somewhere it was not known to, when it agrees somewhere it was known
/// not to, when it stops taking the comparisons it is here for, and when it does not say what it
/// found at all.
pub(crate) fn run(root: &Path) -> Result<()> {
    let cargo = process::cargo();
    println!(
        "verify   {PHASE}     the live window's display list and resolved geometry against a \
         window rebuilt from nothing"
    );
    println!(
        "  green means every saving the incremental engine took was sound, not that either window \
         is right; the known disagreements are listed one at a time in {HERE}"
    );
    for known in KNOWN {
        let (output, ran) = process::capture_outcome(
            root,
            &cargo,
            &[
                "run",
                "--release",
                "-p",
                HARNESS,
                "--bin",
                BINARY,
                "--",
                PHASE,
                known.size,
            ],
        )?;
        print!("{output}");
        let verdict =
            Verdict::read(&output).ok_or_else(|| Error::failed(silent(known, ran, &output)))?;
        verdict.hold_to(known)?;
    }
    Ok(())
}

/// What one run said it found.
struct Verdict {
    /// How many comparisons it took.
    steps: usize,
    /// The steps at which the two windows drew different pictures.
    faults: Vec<usize>,
    /// The steps at which they differed only in the last bits of a float.
    roundings: Vec<usize>,
}

impl Verdict {
    /// Reads the one line a run states its verdict on, or nothing when it never stated one.
    fn read(output: &str) -> Option<Self> {
        let line = output
            .lines()
            .find(|line| line.starts_with("VERIFY size="))?;
        Some(Self {
            steps: field(line, "steps_checked=")?.parse().ok()?,
            faults: steps(field(line, "faults=")?),
            roundings: steps(field(line, "rounded=")?),
        })
    }

    /// Fails unless the run found exactly what it is known to find.
    fn hold_to(&self, known: &Known) -> Result<()> {
        if self.steps != known.steps {
            return Err(Error::failed(format!(
                "verify took {} comparisons at {} and {HERE} says it takes {}. A differential that \
                 stopped comparing is not a differential that agreed: find out which steps went, \
                 and either put them back or say in {HERE} why the document at that size cannot \
                 take them.",
                self.steps, known.size, known.steps,
            )));
        }
        // The unstable steps are taken out of both sides rather than out of the run's answer only:
        // a step that disagrees in one run of ten can neither be asserted to fault nor asserted to
        // agree, and holding it to either turns the whole list into something nobody trusts.
        let faults = held(&self.faults, known.unstable);
        let roundings = held(&self.roundings, known.unstable);
        drifted("fault", known, &faults, &held(known.faults, known.unstable))?;
        drifted(
            "rounding",
            known,
            &roundings,
            &held(known.roundings, known.unstable),
        )?;
        if !known.faults.is_empty() {
            println!("  {} known: {}", known.size, known.because);
        }
        if !known.unstable.is_empty() {
            println!("  {} unstable: {}", known.size, known.unstable_because);
        }
        Ok(())
    }
}

/// The steps of a set that are held exactly, which is all of them but the unstable ones.
fn held(steps: &[usize], unstable: &[usize]) -> Vec<usize> {
    steps
        .iter()
        .copied()
        .filter(|step| !unstable.contains(step))
        .collect()
}

/// Fails when a run's steps of one kind are not the steps it is known to have.
fn drifted(kind: &str, known: &Known, found: &[usize], expected: &[usize]) -> Result<()> {
    let new: Vec<usize> = found
        .iter()
        .copied()
        .filter(|step| !expected.contains(step))
        .collect();
    let gone: Vec<usize> = expected
        .iter()
        .copied()
        .filter(|step| !found.contains(step))
        .collect();
    if new.is_empty() && gone.is_empty() {
        return Ok(());
    }
    let mut message = format!("verify at {}: ", known.size);
    if !new.is_empty() {
        message.push_str(&format!(
            "step {new:?} is a {kind} nothing knew about. Two windows on one document disagreed \
             where nobody has looked. Find out what the disagreement is — `VERIFY_PAIR=<dir>` \
             leaves both display lists in the form they were compared in — and either fix it or \
             add it to {HERE} with what you found. ",
        ));
    }
    if !gone.is_empty() {
        message.push_str(&format!(
            "step {gone:?} is a {kind} {HERE} still expects and the run no longer has. If it was \
             fixed, take the entry out; a budget that outlives what it budgets for is a budget \
             that will absorb the next one silently.",
        ));
    }
    Err(Error::failed(message))
}

/// What to say when a run never stated a verdict at all.
fn silent(known: &Known, ran: bool, output: &str) -> String {
    format!(
        "verify at {} printed no verdict line{}. A gate reads what its subject says it found; a \
         run that says nothing has not agreed, and must not be read as agreeing.\nWhat it printed, \
         last five lines: {}",
        known.size,
        if ran {
            ""
        } else {
            " and exited non-zero, so it did not reach the end of the script"
        },
        output.lines().rev().take(5).collect::<Vec<_>>().join(" | "),
    )
}

/// One `name=value` of a verdict line.
fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|word| word.strip_prefix(name))
}

/// A set of steps as the harness prints it.
fn steps(field: &str) -> Vec<usize> {
    if field == "none" {
        return Vec::new();
    }
    field
        .split(',')
        .filter_map(|step| step.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{KNOWN, Verdict, silent};

    /// A verdict line as the harness prints one.
    const LINE: &str = "VERIFY size=s13 boxes=1865 fragments=2399 steps_checked=95 \
                        faults=62,63 rounded=80 transcript_lines=4";

    /// A budget entry with faults and a rounding in it, for holding the reading logic to.
    ///
    /// Written here rather than borrowed from [`KNOWN`], and that is the point: what these check is
    /// that a run is held to whatever its entry says, which has to stay checkable on the day no
    /// document has a fault left. It did once borrow the real `s13` entry, and the day that entry
    /// went to nothing these tests went with it — a gate whose own tests are switched off by the
    /// defect being fixed is a gate that stops being tested exactly when it starts being trusted.
    fn s13() -> &'static super::Known {
        &super::Known {
            size: "s13",
            steps: 95,
            faults: &[62, 63, 64, 65, 66, 67, 68, 69, 70],
            roundings: &[80],
            unstable: &[],
            because: "the fixture these tests are held to",
            unstable_because: "nothing here is intermittent",
        }
    }

    #[test]
    fn a_run_that_found_what_is_known_passes() {
        let verdict =
            Verdict::read(&LINE.replace("faults=62,63", "faults=62,63,64,65,66,67,68,69,70"))
                .expect("the line is read");
        verdict
            .hold_to(s13())
            .expect("the known faults are the budgeted ones");
    }

    #[test]
    fn a_fault_nobody_knew_about_fails_and_names_the_step() {
        let verdict =
            Verdict::read(&LINE.replace("faults=62,63", "faults=7,62,63,64,65,66,67,68,69,70"))
                .expect("the line is read");
        let message = verdict
            .hold_to(s13())
            .expect_err("an unknown fault fails")
            .to_string();
        assert!(message.contains('7'), "{message}");
        assert!(message.contains("nothing knew about"), "{message}");
        assert!(message.contains("xtask/src/verify/budget.rs"), "{message}");
    }

    #[test]
    fn a_fault_that_was_fixed_fails_until_the_entry_goes() {
        let verdict =
            Verdict::read(&LINE.replace("faults=62,63", "faults=62,63,64,65,66,67,68,69"))
                .expect("the line is read");
        let message = verdict
            .hold_to(s13())
            .expect_err("a fixed fault fails")
            .to_string();
        assert!(message.contains("70"), "{message}");
        assert!(message.contains("take the entry out"), "{message}");
    }

    #[test]
    fn a_new_rounding_fails_as_loudly_as_a_fault() {
        let verdict = Verdict::read(
            &LINE
                .replace("faults=62,63", "faults=62,63,64,65,66,67,68,69,70")
                .replace("rounded=80", "rounded=80,81"),
        )
        .expect("the line is read");
        let message = verdict
            .hold_to(s13())
            .expect_err("a new rounding fails")
            .to_string();
        assert!(message.contains("81"), "{message}");
        assert!(message.contains("rounding"), "{message}");
    }

    #[test]
    fn a_run_that_took_fewer_comparisons_fails_however_much_it_agreed() {
        let verdict = Verdict::read(
            &LINE
                .replace("steps_checked=95", "steps_checked=12")
                .replace("faults=62,63", "faults=none")
                .replace("rounded=80", "rounded=none"),
        )
        .expect("the line is read");
        let message = verdict
            .hold_to(s13())
            .expect_err("a run that stopped comparing fails")
            .to_string();
        assert!(message.contains("12"), "{message}");
        assert!(message.contains("stopped comparing"), "{message}");
    }

    #[test]
    fn a_run_with_no_verdict_line_is_not_read_as_agreement() {
        assert!(Verdict::read("nothing at all\n").is_none());
        let message = silent(s13(), false, "one\ntwo\n");
        assert!(message.contains("has not agreed"), "{message}");
        assert!(message.contains("exited non-zero"), "{message}");
    }

    #[test]
    fn an_unstable_step_passes_whether_it_faulted_or_not() {
        let s1 = KNOWN
            .iter()
            .find(|known| known.size == "s1")
            .expect("s1 is budgeted");
        assert!(
            !s1.unstable.is_empty(),
            "s1 is the size with the open Theme defect"
        );
        for faults in [
            "faults=none",
            "faults=37,38",
            "faults=39,40,41,60,61",
            "faults=70,71",
        ] {
            let line = LINE
                .replace("size=s13", "size=s1")
                // The real `s1` entry counts the whole script, while `LINE` carries the local
                // fixture's shorter count.
                .replace("steps_checked=95", "steps_checked=108")
                .replace("faults=62,63", faults)
                .replace("rounded=80", "rounded=none");
            Verdict::read(&line)
                .expect("the line is read")
                .hold_to(s1)
                .unwrap_or_else(|error| panic!("{faults} is inside the unstable span: {error}"));
        }
    }

    #[test]
    fn a_fault_outside_the_unstable_span_still_fails_at_that_size() {
        let s1 = KNOWN
            .iter()
            .find(|known| known.size == "s1")
            .expect("s1 is budgeted");
        let line = LINE
            .replace("size=s13", "size=s1")
            .replace("steps_checked=95", "steps_checked=108")
            .replace("faults=62,63", "faults=36,37")
            .replace("rounded=80", "rounded=none");
        let message = Verdict::read(&line)
            .expect("the line is read")
            .hold_to(s1)
            .expect_err("step 36 is outside the span")
            .to_string();
        assert!(message.contains("36"), "{message}");
        assert!(!message.contains("37"), "and 37 is not named: {message}");
    }

    #[test]
    fn none_is_read_as_the_empty_set_and_not_as_a_step() {
        let verdict =
            Verdict::read(&LINE.replace("faults=62,63", "faults=none")).expect("the line is read");
        assert!(verdict.faults.is_empty());
        assert_eq!(verdict.roundings, vec![80]);
    }
}
