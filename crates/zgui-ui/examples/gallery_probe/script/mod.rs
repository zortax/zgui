//! What the run does, panel by panel.
//!
//! The order is the order the panels are on the page, with two exceptions at the end. Focus is
//! walked after every overlay has been opened and closed again, because a Tab order that has been
//! left with a dismissed surface still in it is exactly the fault worth catching. The theme is
//! flipped last, because it changes every pixel in the window and would make every capture taken
//! after it incomparable with the ones taken before.

pub(crate) mod atoms;
pub(crate) mod choices;
pub(crate) mod cycles;
pub(crate) mod data;
pub(crate) mod disclosure;
pub(crate) mod drawings;
pub(crate) mod feedback;
pub(crate) mod fields;
pub(crate) mod find;
pub(crate) mod focus;
pub(crate) mod gauntlet;
pub(crate) mod menus;
pub(crate) mod navigation;
pub(crate) mod overlays;
pub(crate) mod scrolling;
pub(crate) mod surfaces;
pub(crate) mod theme;
pub(crate) mod verdict;

use crate::stage::Stage;

/// One part of the run, and what it is called.
pub(crate) type Section = (&'static str, fn(&mut Stage<'_>));

/// The environment variable that narrows a run to the parts named in it, comma separated.
///
/// A run of the whole script takes minutes, and a question about one component is answered by the
/// part that drives it. The opening part is always kept: it is what records where everything is
/// before anything has been touched, and a report without it names boxes nothing can be checked
/// against.
const ONLY: &str = "ZGUI_PROBE_ONLY";

/// The parts of the run, in order, each short enough to finish inside one turn of the loop.
///
/// They are separate entries rather than one function because the driver has to hand the loop back
/// between them. A window that does not return to its event loop stops answering the compositor's
/// liveness ping: the desktop then marks it as not responding, dims it, and offers to kill it — and
/// every capture taken from that moment on is of a stale surface rather than of the frame the
/// application just drew. Which is a way of producing pictures that show the wrong thing while
/// every measurement taken inside the process still looks right.
pub(crate) fn sections() -> Vec<Section> {
    let all = every_section();
    let Ok(wanted) = std::env::var(ONLY) else {
        return all;
    };
    let wanted: Vec<&str> = wanted.split(',').map(str::trim).collect();
    all.into_iter()
        .filter(|(name, _)| *name == "opening" || wanted.contains(name))
        .collect()
}

/// Every part, in order.
fn every_section() -> Vec<Section> {
    let mut sections = opening_sections();
    // One cycle, or one step, per entry: see [`gauntlet`] for why these are not one part each.
    sections.extend(core::iter::repeat_n(
        (
            "gauntlet-modals",
            gauntlet::modals::chunk as fn(&mut Stage<'_>),
        ),
        gauntlet::modals::STEPS,
    ));
    sections.extend(core::iter::repeat_n(
        (
            "gauntlet-nested",
            gauntlet::nested::chunk as fn(&mut Stage<'_>),
        ),
        gauntlet::nested::STEPS,
    ));
    sections.extend(core::iter::repeat_n(
        (
            "gauntlet-endurance",
            gauntlet::endurance::chunk as fn(&mut Stage<'_>),
        ),
        gauntlet::endurance::STEPS,
    ));
    sections.extend(closing_sections());
    sections
}

/// Everything up to and including the parts that open each component once.
fn opening_sections() -> Vec<Section> {
    let mut opening: Vec<Section> = vec![
        ("opening", opening),
        ("atoms", atoms::run),
        ("fields", fields::run),
        ("choices", choices::run),
        ("feedback", feedback::run),
        ("disclosure", disclosure::run),
        ("overlays", overlays::run),
        ("cycles", cycles::run),
        ("menus", menus::run),
        ("navigation", navigation::run),
        ("surfaces", surfaces::run),
        ("scrolling", scrolling::run),
        ("data", data::run),
        ("drawings", drawings::run),
    ];
    // The parts that judge pictures come after the parts that open everything once, because each of
    // them leaves the window in a state of its own — a field emptied, a stack of messages, a dialog
    // open — and a part that opened a component for the first time in one of those states would be
    // reporting on the state as much as on the component.
    opening.extend(verdict::sections());
    opening
}

/// The parts that run after the window has been driven hard, and the last capture.
///
/// Focus is walked at the end, because a Tab order left with a dismissed surface still in it is
/// exactly the fault worth catching after a hundred of them have come and gone. The theme is
/// flipped last, because it changes every pixel in the window.
fn closing_sections() -> Vec<Section> {
    vec![
        ("focus", focus::run),
        ("theme", theme::run),
        ("closing", |stage| stage.shot("99-finished")),
    ]
}

/// What the run records before it touches anything.
fn opening(stage: &mut Stage<'_>) {
    let census = stage.census();
    let scale = stage.scale();
    let focusables = {
        let handles = stage.handles().clone();
        handles.host.focusables(handles.root()).len()
    };
    stage.report.note(
        "document",
        &format!(
            "{} nodes, {focusables} of them focusable, at {scale} device pixels per CSS pixel",
            census.len()
        ),
    );
    // Every box, where it is and what it says, written out beside the first capture. Without it a
    // step that lands on the wrong thing and a control that does not answer are the same report.
    for node in &census.nodes {
        if let Some(rect) = node.rect
            && rect.size.width.0 > 0.0
        {
            stage.report.rect(
                &format!("node:{}", node.text.chars().take(40).collect::<String>()),
                rect.origin.x.0,
                rect.origin.y.0,
                rect.size.width.0,
                rect.size.height.0,
            );
        }
    }
    stage.shot("00-opened");
}
