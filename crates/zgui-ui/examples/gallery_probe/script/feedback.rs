//! Alerts, cards, progress and the toaster.
//!
//! The indeterminate progress bar is the one thing here that has to be watched over time rather
//! than looked at once: it is an animation, and an animation that has stopped looks exactly like
//! one that never started. So it is left running for a stretch of real seconds and asked, at the
//! end, both how many frames went by and whether the thing that moves has moved.

use core::time::Duration;

use crate::script::find;
use crate::stage::Stage;

/// How long the indeterminate bar is watched.
const WATCH: Duration = Duration::from_millis(1500);

/// Drives the feedback components.
pub(crate) fn run(stage: &mut Stage<'_>) {
    still_things(stage);
    progress(stage);
    toasts(stage);
}

/// Alerts and cards, which have no behaviour beyond being laid out.
fn still_things(stage: &mut Stage<'_>) {
    let census = stage.census();
    if let Some(panel) = find::mark_panel(stage, &census, "Alert") {
        let both = ["Heads up", "Your card expires this month"]
            .iter()
            .filter(|title| find::at_in(&census, panel, title).is_some())
            .count();
        stage.report.check(
            "Alert",
            "both tones are laid out",
            both == 2,
            &format!("{both} of 2"),
        );
        stage.shot("feedback-alerts");
    }
    let census = stage.census();
    if let Some(panel) = find::mark_panel(stage, &census, "Card") {
        let parts = ["March", "Due on the 28th", "£42.00 for one seat.", "Pay"]
            .iter()
            .filter(|text| find::at_in(&census, panel, text).is_some())
            .count();
        stage.report.check(
            "Card",
            "the header, body and footer are all laid out",
            parts == 4,
            &format!("{parts} of 4 parts"),
        );
        stage.shot("feedback-card");
    }
}

/// The progress bars, determinate and not.
fn progress(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Progress") else {
        stage.report.note("Progress", "the panel is not laid out");
        return;
    };
    let reading = |stage: &Stage<'_>| -> Option<String> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| node.text.ends_with('%'))
            .min_by(|left, right| left.text.len().cmp(&right.text.len()))
            .map(|node| node.text.clone())
    };
    stage.report.check(
        "Progress",
        "the determinate bar starts where it was put",
        reading(stage).as_deref() == Some("62%"),
        &format!("it reads {:?}", reading(stage)),
    );

    // Every wide flat box in the panel, so that a fraction read off the wrong one is a fraction
    // that can be seen to have come off the wrong one.
    let mut boxes = Vec::new();
    for node in stage.census().inside(panel) {
        if let Some(rect) = node.rect
            && rect.size.width.0 > 8.0
        {
            boxes.push(format!(
                "{:?} {:.0},{:.0} {:.0}x{:.0}",
                node.text.chars().take(16).collect::<String>(),
                rect.origin.x.0,
                rect.origin.y.0,
                rect.size.width.0,
                rect.size.height.0
            ));
        }
    }
    stage
        .report
        .note("Progress", &format!("the panel holds {boxes:?}"));

    // The filled part of the bar is a box, so how far along it is can be measured rather than
    // taken on trust from the number beside it.
    let filled = filled_fraction(stage);
    stage.report.check(
        "Progress",
        "the bar is filled to the value it reports",
        filled.is_some_and(|fraction| (fraction - 0.62).abs() < 0.06),
        &format!("the filled part is {filled:?} of the track, and the value is 0.62"),
    );
    stage.shot("feedback-progress-62");

    if let Some(more) = find::at_in(&census, panel, "More") {
        stage.click(more);
        stage.click(more);
        // The bar reaches its new value over a transition, in real time. Measured in the frame
        // after the click it is caught part of the way there, and a bar travelling correctly
        // towards the right number reports as one that stopped at the wrong one.
        stage.wait(Duration::from_millis(500));
        let after = reading(stage);
        stage.report.check(
            "Progress",
            "the buttons move it",
            after.as_deref() == Some("82%"),
            &format!("two presses of More gave {after:?}"),
        );
        let filled = filled_fraction(stage);
        stage.report.check(
            "Progress",
            "the bar follows the value",
            filled.is_some_and(|fraction| (fraction - 0.82).abs() < 0.06),
            &format!("the filled part is now {filled:?}"),
        );
        stage.shot("feedback-progress-82");
    }

    // The indeterminate one. Nothing is clicked; it either animates by itself or it does not.
    //
    // Where the moving part has got to cannot be read from here: it travels by transform, which
    // moves what is drawn and not the box that was laid out, so the box is in the same place at
    // every moment of the journey. Reading it and finding it unmoved is what a bar that never
    // started and a bar half way along have in common, and reporting the second as the first is
    // reporting a working animation as a broken one. What the engine will say is whether the
    // animation is running, so that is what is asked, over a stretch of real seconds so that one
    // that stops after a cycle is not mistaken for one that keeps going.
    let track = track_below(stage, "indeterminate");
    let fill = track.and_then(|track| fill_in(stage, track));
    stage.report.note(
        "Progress",
        &format!("the indeterminate track is {track:?} and its moving part {fill:?}"),
    );
    let before = fill.map(|node| stage.animations(node));
    let frames = stage.wait(WATCH);
    let after = fill.map(|node| stage.animations(node));
    stage.report.check(
        "Progress",
        "the indeterminate bar animates",
        before.is_some_and(|count| count > 0) && after.is_some_and(|count| count > 0),
        &format!(
            "over {WATCH:?} and {frames} frames the moving part had {before:?} animations running \
             and then {after:?}"
        ),
    );
    stage.shot("feedback-progress-indeterminate");
}

/// The wide flat box that is the track under the label saying `label`.
///
/// By the label above it rather than by being the widest flat box in the panel. There are two
/// tracks here of exactly the same size, so "the widest" is a tie broken by iteration order — and
/// a claim about the determinate bar answered from the indeterminate one is a claim about nothing
/// that reads like a defect in the bar it was never looking at.
///
/// The label is looked for in the whole document rather than inside a rectangle measured earlier.
/// A rectangle is a photograph of where a panel was: every click, every wait and every frame since
/// can have moved the page under it, and a panel that has moved by one pixel no longer *contains*
/// its own contents by the test [`Census::inside`](crate::stage::census::Census::inside) applies.
/// The bar that is animating perfectly well is then simply not in the region being looked at, and
/// the run says the component is broken. What the label and its track have that no rectangle can
/// lose is each other: the track is the flat box immediately under the words.
fn track_below(
    stage: &Stage<'_>,
    label: &str,
) -> Option<(
    zgui::view::NodeId,
    zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>,
)> {
    let census = stage.census();
    let heading = census
        .saying(label)
        .into_iter()
        .filter(|node| node.area() > 0.0)
        .filter_map(|node| node.rect)
        .min_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        })?;
    let below = heading.origin.y.0 + heading.size.height.0;
    census
        .nodes
        .iter()
        .filter(|node| node.text.is_empty() && census.on_the_page(node))
        .filter_map(|node| node.rect.map(|rect| (node.id, rect)))
        .filter(|(_, rect)| {
            // Under the words, starting at the same edge, and flat: a track is many times wider
            // than it is tall, which no other empty box in a panel of bars is.
            rect.origin.y.0 >= below - 0.5
                && (rect.origin.x.0 - heading.origin.x.0).abs() < 2.0
                && rect.size.width.0 > rect.size.height.0 * 8.0
        })
        .min_by(|(_, left), (_, right)| left.origin.y.0.total_cmp(&right.origin.y.0))
}

/// The filled part inside the track `track`.
fn fill_in(
    stage: &Stage<'_>,
    (track, _rect): (
        zgui::view::NodeId,
        zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>,
    ),
) -> Option<zgui::view::NodeId> {
    let census = stage.census();
    // The track's own descendants, asked of the tree rather than of a rectangle. The fill was
    // once looked for inside the track's *box*, and a box is a photograph: a page that moves
    // between the measuring and the looking leaves the fill outside the rectangle it is looked
    // for in, and the moving part comes back as not found. Whose child the fill is cannot move.
    let inside: Vec<&crate::stage::census::Seen> = census
        .nodes
        .iter()
        .filter(|node| {
            node.text.is_empty()
                && node.area() > 0.0
                && node.id != track
                && stage.handles().host.contains(track, node.id)
        })
        .collect();
    // The widest one narrower than the track, when there is one: a track can carry a same-size
    // twin inside it, and the fill is the only thing in there that is shorter than the whole.
    // When nothing is narrower — a fill laid out at full width and sent along by transform — the
    // innermost descendant is the fill, and document order puts it last.
    inside
        .iter()
        .filter(|node| {
            node.rect.is_some_and(|own| {
                census
                    .node(track)
                    .and_then(|track| track.rect)
                    .is_some_and(|track| own.size.width.0 < track.size.width.0 - 0.5)
            })
        })
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .copied()
        .or_else(|| inside.last().copied())
        .map(|node| node.id)
}

/// How much of the determinate track is filled, from zero to one.
fn filled_fraction(stage: &Stage<'_>) -> Option<f32> {
    let track = track_below(stage, "determinate")?;
    let fill = fill_in(stage, track)?;
    let census = stage.census();
    census
        .node(fill)
        .and_then(|node| node.rect)
        .map(|rect| rect.size.width.0 / track.1.size.width.0.max(1.0))
}

/// The toaster, which is the one surface nothing on the page holds.
fn toasts(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Toast") else {
        stage.report.note("Toast", "the panel is not laid out");
        return;
    };
    let Some(save) = find::at_in(&census, panel, "Save") else {
        stage.report.note("Toast", "no Save button");
        return;
    };
    stage.click(save);
    stage.settle(6);
    let after = stage.census();
    let showing = after
        .nodes
        .iter()
        .any(|node| node.text == "Your changes are on the server." && node.area() > 0.0);
    stage.report.check(
        "Toast",
        "announcing one puts it on the screen",
        showing,
        "the toast's description is laid out",
    );
    stage.shot("feedback-toast");

    // A second one, of a different kind, has to stack rather than replace.
    let census = stage.census();
    if let Some(fail) = find::at_in(&census, panel, "Fail") {
        stage.click(fail);
        stage.settle(6);
        let after = stage.census();
        let both = after
            .nodes
            .iter()
            .filter(|node| {
                (node.text == "Your changes are on the server."
                    || node.text == "The server said no.")
                    && node.area() > 0.0
            })
            .count();
        stage.report.check(
            "Toast",
            "a second one stacks with the first",
            both == 2,
            &format!("{both} toasts are on the screen"),
        );
        stage.shot("feedback-toast-two");
    }
}
