//! Whether the keyboard can reach everything, and whether it stays where it is put.
//!
//! The claim that matters is not that Tab moves focus — it is that Tab moves focus *somewhere new*
//! every time, in the order things are on the page, and that it eventually comes back round. A
//! sequence that visits three controls and then sticks satisfies "Tab works" and is unusable.

use zgui::vocab::{Modifiers, NamedKey};

use crate::script::find;
use crate::stage::Stage;

/// How many tabs to take. More than the gallery has controls, so the sequence has to wrap.
const STEPS: usize = 90;

/// Drives the focus checks.
pub(crate) fn run(stage: &mut Stage<'_>) {
    reachable(stage);
    walk(stage);
}

/// What the document says is focusable, against what is actually there.
fn reachable(stage: &mut Stage<'_>) {
    let handles = stage.handles().clone();
    let focusables = handles.host.focusables(handles.root());
    let census = stage.census();
    let without_a_box = focusables
        .iter()
        .filter(|node| census.node(**node).is_none_or(|seen| seen.area() <= 0.0))
        .count();
    stage.report.check(
        "Focus",
        "everything in the focus order is laid out",
        without_a_box == 0,
        &format!(
            "{} focusable nodes, {without_a_box} of which have no box",
            focusables.len()
        ),
    );
    stage.report.check(
        "Focus",
        "the page has as many controls as it shows",
        focusables.len() >= 60,
        &format!("{} focusable nodes", focusables.len()),
    );
}

/// Tab, over and over, watching where it goes.
fn walk(stage: &mut Stage<'_>) {
    // Start from a known place rather than from wherever the last panel left it.
    let census = stage.census();
    if let Some(masthead) = census
        .control("zgui components")
        .and_then(|node| node.centre())
    {
        stage.click(masthead);
    }

    let mut seen = Vec::new();
    let mut stuck = 0;
    let mut nowhere = 0;
    let mut previous = stage.focused();
    for _ in 0..STEPS {
        stage.key(NamedKey::Tab);
        let now = stage.focused();
        match now {
            None => nowhere += 1,
            Some(node) => {
                if Some(node) == previous {
                    stuck += 1;
                }
                if !seen.contains(&node) {
                    seen.push(node);
                }
            }
        }
        previous = now;
    }
    stage.report.check(
        "Focus",
        "Tab keeps moving rather than sticking",
        stuck == 0,
        &format!("{STEPS} tabs stayed put {stuck} times and landed nowhere {nowhere} times"),
    );
    stage.report.check(
        "Focus",
        "Tab reaches a large part of the page",
        seen.len() >= 40,
        &format!("{STEPS} tabs visited {} distinct controls", seen.len()),
    );
    stage.shot("focus-tabbed");

    // Backwards, which is a separate order and is where an off-by-one shows.
    let forward = stage.focused();
    stage.key_with(NamedKey::Tab, Modifiers::SHIFT);
    let back_one = stage.focused();
    stage.key_with(NamedKey::Tab, Modifiers::SHIFT);
    stage.key(NamedKey::Tab);
    let forward_again = stage.focused();
    stage.report.check(
        "Focus",
        "Shift+Tab goes back the way Tab came",
        back_one.is_some() && back_one != forward && forward_again == back_one,
        &format!("forward at {forward:?}, back to {back_one:?}, and back-back-forward to {forward_again:?}"),
    );

    // A focus ring has to be something the window draws, so the capture is marked with where the
    // focused control is and compared against the same control unfocused.
    if let Some(node) = stage.focused()
        && let Some(rect) = stage.census().node(node).and_then(|seen| seen.rect)
    {
        find::mark(stage, "focus:ring", rect);
        stage.shot("focus-ring-on");
        stage.click(zgui::geom::Point::new(
            zgui::geom::DevicePx(rect.origin.x.0),
            zgui::geom::DevicePx(rect.origin.y.0 - 40.0),
        ));
        stage.shot("focus-ring-off");
    }
}
