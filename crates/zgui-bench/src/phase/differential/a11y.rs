//! `a11y-geom`: the rectangles this window hands to things outside the process.
//!
//! Two consumers are told where something is, and neither of them can see the display list. A
//! screen magnifier is given a rectangle per accessibility node and draws a frame around it; an
//! input method is given the caret's rectangle and opens its candidate window beside it. Both are
//! read out of the same fragments the picture is drawn from and both have to travel through the
//! coordinate system the fragment was measured in — and if either stops doing that, the window
//! looks perfect and the magnifier highlights the wrong part of the screen. That is why the
//! rectangles are worth a gate of their own; which half of the danger this particular gate covers
//! is the section below, and it is the smaller half.
//!
//! So the same script is driven over a live window and one holding nothing, and after every step
//! both are asked for a whole tree and for where they would put a candidate window.
//!
//! # What a green run means, and what it does not
//!
//! **It means the rectangles a running window publishes are the rectangles a rebuild publishes.**
//! The live window's placements, its projection's dirty set and the resolved lines a caret is
//! placed along are all carried forward frame to frame, and a piece of that left stale is a
//! consumer told about a control that has since moved — which is a difference from a rebuild, and
//! is what this sees.
//!
//! **It does not mean a published rectangle is correct.** Both windows resolve through the same
//! `project::geometry::bounds_of`, so an error there is made twice and cancels. Leaving bounds
//! unresolved — `Placements::EMPTY`, every control reported where it would be if nothing above it
//! had moved, which is exactly the failure the opening paragraph describes — leaves this gate green
//! at every size. It is caught instead by
//! `a_control_under_a_transform_is_reported_where_the_transform_puts_it` in
//! `zgui-runtime/tests/a11y.rs`, and the caret half by
//! `an_input_method_is_told_where_the_caret_is_drawn_and_not_where_it_was_measured` in
//! `zgui-runtime/tests/editing.rs`. Both are named beside this gate in
//! `xtask/src/oracle/subject.rs` and checked to exist before it runs.
//!
//! # Why a whole tree, and not the update the frame published
//!
//! An update is a *difference*: it names the nodes that changed since the last one. The thorough
//! window recomputes everything, so it has more to say every frame and would report nodes the live
//! window correctly left out. What is comparable is the state, not the difference, so both windows
//! are asked for everything they hold — which is exactly what a consumer that has just connected
//! is given.

use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::phase::Driver;
use crate::phase::differential::twin::Twin;
use crate::script::script;

/// Everything one window told a consumer about where things are.
struct Told {
    /// One line per accessibility node, in the order the projection emitted them.
    nodes: Vec<String>,
    /// How many of those carry a rectangle.
    placed: usize,
    /// Where the surface was last told to put a candidate window.
    caret: Option<String>,
}

/// Asks one window for a whole tree, and for where it would put a candidate window.
///
/// # Panics
///
/// Panics when the surface was handed no tree, which is a window that published nothing rather
/// than a window that published something empty.
fn told(harness: &mut Harness<Runtime>) -> Told {
    harness.app_mut().windows_mut()[0].publish_full_a11y_tree();
    let surface = harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .clone();
    let update = surface
        .last_a11y_update()
        .expect("the window published a tree");
    let mut nodes = Vec::with_capacity(update.nodes.len());
    let mut placed = 0;
    for (_, node) in &update.nodes {
        // The identifier is deliberately left out: the two windows are two documents and mint
        // different ones for the same element. What is compared is what the node says about where
        // it is, in the order the projection walked to it.
        let bounds = match node.bounds() {
            Some(rect) => {
                placed += 1;
                format!(
                    "{:.4} {:.4} {:.4} {:.4}",
                    rect.x0, rect.y0, rect.x1, rect.y1
                )
            }
            None => "-".to_owned(),
        };
        let transform = node.transform().map_or_else(
            || "-".to_owned(),
            |affine| format!("{:?}", affine.as_coeffs()),
        );
        nodes.push(format!("{:?} {bounds} {transform}", node.role()));
    }

    // What the surface was *told*, rather than what the window would answer now. The two differ
    // for the thorough window and only for it: it is left holding nothing between frames, and the
    // resolved lines a caret is placed along are among the things it threw away — so asking it
    // afterwards is asking a window that has forgotten the frame it drew. The area the input method
    // was handed was taken while that frame was being made, which is the state being compared.
    let caret = surface.last_text_input().flatten().map(|input| {
        format!(
            "caret {:.4} {:.4} {:.4} {:.4}",
            input.caret_origin.x.0,
            input.caret_origin.y.0,
            input.caret_size.width.0,
            input.caret_size.height.0,
        )
    });
    Told {
        nodes,
        placed,
        caret,
    }
}

/// What the run has found so far.
#[derive(Default)]
struct Tally {
    /// Steps where the two windows disagreed.
    faults: usize,
    /// Steps compared.
    steps: usize,
    /// Steps whose windows had already laid the document out differently.
    apart: usize,
    /// The most nodes carrying a rectangle any one step reported.
    placed: usize,
    /// Steps at which a caret rectangle existed at all.
    carets: usize,
}

/// Compares one step, reporting what it found.
///
/// # Panics
///
/// Panics when a step's trees carry so few rectangles that agreeing about them would mean nothing.
fn compare(step: usize, what: &str, driver: &mut Driver, twin: &mut Twin, tally: &mut Tally) {
    twin.settle(&mut driver.harness);
    if !twin.laid_out_alike(&driver.harness) {
        tally.apart += 1;
        println!("  A11Y skip step {step} ({what}): the two windows are not laid out alike");
        return;
    }
    let live = told(&mut driver.harness);
    let cold = told(&mut twin.cold);

    // The control, in the same shape as the display list's: two trees that say nothing about where
    // anything is agree perfectly, and so would a projection that had stopped reporting geometry
    // altogether.
    assert!(
        live.placed >= 2,
        "step {step} ({what}): {} nodes carry a rectangle, so nothing is being compared",
        live.placed,
    );
    tally.steps += 1;
    tally.placed = tally.placed.max(live.placed);
    tally.carets += usize::from(live.caret.is_some());

    let mut differing = live.nodes.len().abs_diff(cold.nodes.len());
    let mut first = (live.nodes.len() != cold.nodes.len())
        .then(|| format!("node count {} vs {}", live.nodes.len(), cold.nodes.len()));
    for (index, (one, two)) in live.nodes.iter().zip(cold.nodes.iter()).enumerate() {
        if one != two {
            differing += 1;
            if first.is_none() {
                first = Some(format!("node {index}: live {one:?} cold {two:?}"));
            }
        }
    }
    if live.caret != cold.caret {
        differing += 1;
        first.get_or_insert(format!(
            "caret: live {:?} cold {:?}",
            live.caret, cold.caret
        ));
    }

    if differing == 0 {
        println!(
            "  A11Y ok   step {step} ({what}), {} nodes, {} placed",
            live.nodes.len(),
            live.placed
        );
    } else {
        tally.faults += 1;
        println!(
            "  A11Y FAULT step {step} ({what}): {differing} differ [{}]",
            first.as_deref().unwrap_or("-"),
        );
    }
}

/// Runs the phase, or answers `None` when the name is not this one.
///
/// # Panics
///
/// Panics when the two windows told a consumer different things.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    if phase != "a11y-geom" {
        return None;
    }
    let size = driver.size.clone();
    let centres = driver.centres.clone();
    let scheme = driver.scheme;
    let mut twin = Twin::open(&size, &mut driver.harness, scheme, &centres);

    let mut tally = Tally::default();
    compare(0, "settled", driver, &mut twin, &mut tally);
    for (index, step) in script().iter().enumerate() {
        twin.step(&mut driver.harness, step);
        compare(
            index + 1,
            &format!("{step:?}"),
            driver,
            &mut twin,
            &mut tally,
        );
    }

    let verdict = if tally.faults == 0 {
        "ok "
    } else {
        "REGRESSION"
    };
    println!(
        "a11y_and_caret_geometry_agree_with_a_cold_window {verdict} size={size} compared={} \
         apart={} placed={} caret_steps={} faults={}",
        tally.steps, tally.apart, tally.placed, tally.carets, tally.faults,
    );
    assert!(
        tally.steps > tally.apart,
        "more steps were skipped than compared, so this says more about the layout the two \
         windows arrived at than about what either of them published",
    );
    // The caret half states itself separately, and only where there was a caret. A document with
    // no editable element in it — the shell on its own is one — has no insertion point to place at
    // any point of the script, and a run that claimed to have compared one would be claiming to
    // have compared `None` against `None` ninety-five times. Which document sizes owe this line is
    // recorded where the gate that reads it is, so a size that quietly stopped planning carets
    // fails rather than passes.
    if tally.carets > 0 {
        println!(
            "caret_geometry_agrees_with_a_cold_window {verdict} size={size} caret_steps={}",
            tally.carets,
        );
    }
    assert_eq!(
        tally.faults, 0,
        "the two windows told a consumer different things at {} steps",
        tally.faults
    );
    Some(0)
}
