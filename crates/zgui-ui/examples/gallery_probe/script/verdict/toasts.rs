//! Toasts: where they go, how they stack, what happens past the limit, and how one leaves.
//!
//! Four turns of the loop, because a toast has a deadline of its own and everything asked here has to
//! happen inside it. The pointer is left resting on the stack at the end of every turn: a stack under
//! the pointer holds its deadlines, so what the run does between two turns cannot be the reason a
//! message went away.
//!
//! Where each one is comes from the boxes rather than from the pictures, and the boxes are read back
//! by handle rather than by taking another census — a census walks the whole arena, and a thing that
//! is measured once per frame has to cost less than the frame does. The pictures are taken beside the
//! numbers so that a stack whose boxes are in the right places and whose paint is not can still be
//! seen.

use core::cell::{Cell, RefCell};
use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::view::NodeId;

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;

use crate::stage::Stage;

/// How many turns of the loop this takes.
pub(crate) const STEPS: usize = 4;

/// What the button that announces one says.
const ANNOUNCE: &str = "Save";

/// Everything one of those toasts says, which is how its box is found.
///
/// No dismiss glyph at the end: the close control draws an icon, and an icon says nothing a
/// census can read.
const SAID: &str = "SavedYour changes are on the server.";

/// How many toasts the toaster is allowed to show at once.
const LIMIT: usize = 3;

/// How many frames to watch after a dismissal, and how long each is given.
///
/// Forty-five frames is three quarters of a second: the exit itself runs four hundred
/// milliseconds, and the row's box goes only when the departure settles after it — a watch that
/// ended sooner would read a perfectly ordinary exit as a toast that never left.
const WATCH: usize = 45;

/// How long the stack is left alone to see whether a hovered one stays.
const HELD: Duration = Duration::from_millis(900);

/// How far apart two boxes can be and still be the same message, in device pixels.
const TOGETHER: f32 = 28.0;

/// How long the fan-out is given after the pointer lands on the stack.
///
/// The slots move over a four-hundred-millisecond transition, so anything aimed at a toast before
/// it has stopped is aimed at where it was mid-flight.
const EXPANDED: Duration = Duration::from_millis(600);

/// How long an entrance or an exit is given before anything is measured.
///
/// A toast enters over a hundred and eighty milliseconds by sliding in, and the box a driver reads
/// back is mapped through that motion — so a stack measured immediately after a push is a stack
/// whose newest member is still on its way and still overlapping the one below it. Everything asked
/// here is about where they come to rest.
const SETTLED: Duration = Duration::from_millis(340);

thread_local! {
    /// Which turn this is.
    static CURSOR: Cell<usize> = const { Cell::new(0) };
    /// Where the button that announces one is, found once and kept.
    static BUTTON: RefCell<Option<Point<DevicePx, Device>>> = const { RefCell::new(None) };
}

/// Runs one turn.
pub(crate) fn chunk(stage: &mut Stage<'_>) {
    let step = CURSOR.with(|cursor| {
        let at = cursor.get();
        cursor.set(at + 1);
        at
    });
    match step {
        0 => first(stage),
        1 => stacked(stage),
        2 => past_the_limit(stage),
        _ => dismissed(stage),
    }
}

/// The button that announces one, revealing its panel the first time it is asked for.
fn button(stage: &mut Stage<'_>) -> Option<Point<DevicePx, Device>> {
    if let Some(at) = BUTTON.with(|button| *button.borrow()) {
        return Some(at);
    }
    let (_, panel) = find::open_panel(stage, "Toast")?;
    // The census is taken again after the reveal, because revealing scrolls: the rectangle the
    // panel was found through names where it stood before the scroll, and a button aimed through
    // it is pressed where the panel used to be.
    let census = stage.census();
    let panel = census.panel("Toast").and_then(|node| node.rect).unwrap_or(panel);
    let at = find::at_in(&census, panel, ANNOUNCE)?;
    BUTTON.with(|button| *button.borrow_mut() = Some(at));
    Some(at)
}

/// Every toast on the screen, nearest the corner first, with its box.
///
/// The toaster is in the bottom trailing corner, so "nearest the corner" is furthest down the window,
/// and the newest is the one that should be there.
fn stack(stage: &Stage<'_>) -> Vec<(NodeId, Rect<DevicePx, Device>)> {
    let census = stage.census();
    let wide = stage.window().size.width.0 / 2.0;
    // One message is three boxes that say the same thing: the region the whole stack is in, the slot
    // that holds one message's place against the corner, and the message itself. The region is thrown
    // out by its width, and the slot and the message are the same message — a few pixels apart,
    // because a slot carries the gap to the next one — so they are grouped by where they are and the
    // outer of the two is kept.
    let candidates: Vec<(NodeId, Rect<DevicePx, Device>)> = census
        .nodes
        .iter()
        .filter(|node| node.text == SAID)
        .filter(|node| node.area() > 0.0)
        .filter_map(|node| node.rect.map(|rect| (node.id, rect)))
        .filter(|(_, rect)| rect.size.width.0 <= wide)
        .collect();
    let mut found: Vec<(NodeId, Rect<DevicePx, Device>)> = Vec::new();
    for (id, rect) in &candidates {
        // The region says everything its one toast says and now has a box of its own — it is what
        // holds the pointer for the stack — so any candidate that wholly contains another
        // candidate is a container, not a message.
        let contains_another = candidates.iter().any(|(other, inner)| {
            other != id
                && inner.origin.x.0 >= rect.origin.x.0 - 0.5
                && inner.origin.y.0 >= rect.origin.y.0 - 0.5
                && inner.origin.x.0 + inner.size.width.0 <= rect.origin.x.0 + rect.size.width.0 + 0.5
                && inner.origin.y.0 + inner.size.height.0
                    <= rect.origin.y.0 + rect.size.height.0 + 0.5
                && (inner.size.width.0 < rect.size.width.0
                    || inner.size.height.0 < rect.size.height.0)
        });
        if contains_another {
            continue;
        }
        match found
            .iter_mut()
            .find(|(_, other)| (other.origin.y.0 - rect.origin.y.0).abs() < TOGETHER)
        {
            Some(existing) => {
                if rect.size.height.0 > existing.1.size.height.0 {
                    *existing = (*id, *rect);
                }
            }
            None => found.push((*id, *rect)),
        }
    }
    found.sort_by(|left, right| right.1.origin.y.0.total_cmp(&left.1.origin.y.0));
    found
}

/// The boxes of `nodes` as they stand now, read straight from the engine.
fn boxes(stage: &Stage<'_>, nodes: &[NodeId]) -> Vec<Option<Rect<DevicePx, Device>>> {
    nodes
        .iter()
        .map(|node| stage.handles().host.window_box(*node))
        .collect()
}

/// How `rect` reads in a report.
fn wrote(rect: &Rect<DevicePx, Device>) -> String {
    format!(
        "{:.0},{:.0} {:.0}x{:.0}",
        rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
    )
}

/// Rests the pointer on the toast nearest the corner, which holds every deadline in the stack.
fn hold(stage: &mut Stage<'_>) {
    if let Some((_, rect)) = stack(stage).first() {
        stage.move_to(Point::new(
            DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
            DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
        ));
    }
}

/// One toast, announced from a button below the corner it appears in.
fn first(stage: &mut Stage<'_>) {
    let Some(at) = button(stage) else {
        stage
            .report
            .note("Toast", "the Toast panel is not laid out");
        return;
    };
    stage.click(at);
    stage.wait(SETTLED);
    let shown = stack(stage);
    stage.report.check(
        "Toast",
        "announcing one puts it on the screen",
        shown.len() == 1,
        &format!(
            "{} on the screen: {}",
            shown.len(),
            shown
                .iter()
                .map(|(_, rect)| wrote(rect))
                .collect::<Vec<_>>()
                .join("  ")
        ),
    );
    if let Some((_, rect)) = shown.first() {
        let window = stage.window();
        stage.report.check(
            "Toast",
            "it appears in the bottom trailing corner",
            rect.origin.x.0 + rect.size.width.0 > window.size.width.0 / 2.0
                && rect.origin.y.0 + rect.size.height.0 > window.size.height.0 / 2.0,
            &format!("it is at {} of a {} window", wrote(rect), wrote(&window)),
        );
        find::mark(stage, "toast:1", *rect);
    }
    shot_of(stage, "vd-toast-1", stage.window());
    hold(stage);
}

/// Three of them, and where each sits relative to the others.
fn stacked(stage: &mut Stage<'_>) {
    let Some(at) = button(stage) else { return };
    stage.click(at);
    stage.wait(SETTLED);
    stage.click(at);
    stage.wait(SETTLED);
    // Clicking the announce button walked the pointer off the stack, which collapses it into a
    // deck — and everything below asks about the fanned-out stack a reader is looking at. So the
    // pointer is put back first, and the fan given its transition.
    hold(stage);
    stage.wait(EXPANDED);
    let shown = stack(stage);
    stage.report.check(
        "Toast",
        "three of them are all on the screen",
        shown.len() == LIMIT,
        &format!(
            "{} on the screen: {}",
            shown.len(),
            shown
                .iter()
                .map(|(_, rect)| wrote(rect))
                .collect::<Vec<_>>()
                .join("  ")
        ),
    );
    // Nearest the corner first, so each one's top edge must be below the next one's bottom edge.
    let clear = shown
        .windows(2)
        .all(|pair| pair[1].1.origin.y.0 + pair[1].1.size.height.0 <= pair[0].1.origin.y.0 + 0.5);
    stage.report.check(
        "Toast",
        "no two of them overlap",
        shown.len() > 1 && clear,
        &format!(
            "top edges at {}",
            shown
                .iter()
                .map(|(_, rect)| format!("{:.0}", rect.origin.y.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    for (index, (_, rect)) in shown.iter().enumerate() {
        find::mark(stage, &format!("toast:3:{index}"), *rect);
    }
    shot_of(stage, "vd-toast-3", stage.window());

    // The whole stack has to stay while the pointer is on it: without that, everything measured
    // after this point is measured on a stack that is quietly emptying itself.
    hold(stage);
    stage.wait(HELD);
    let after = stack(stage);
    stage.report.check(
        "Toast",
        "the stack stays while the pointer is on it",
        after.len() == shown.len(),
        &format!(
            "{} of {} are still there after {HELD:?}",
            after.len(),
            shown.len()
        ),
    );
}

/// A fourth one, which the toaster is not allowed to show beside the other three.
fn past_the_limit(stage: &mut Stage<'_>) {
    let Some(at) = button(stage) else { return };
    let before = stack(stage);
    stage.click(at);
    stage.wait(SETTLED);
    stage.wait(SETTLED);
    let shown = stack(stage);
    stage.report.check(
        "Toast",
        "a fourth one does not make four on the screen",
        shown.len() <= LIMIT,
        &format!(
            "{} were up, {} are up now: {}",
            before.len(),
            shown.len(),
            shown
                .iter()
                .map(|(_, rect)| wrote(rect))
                .collect::<Vec<_>>()
                .join("  ")
        ),
    );
    let clear = shown
        .windows(2)
        .all(|pair| pair[1].1.origin.y.0 + pair[1].1.size.height.0 <= pair[0].1.origin.y.0 + 0.5);
    stage.report.check(
        "Toast",
        "the four-deep stack still does not overlap itself",
        clear,
        &format!(
            "top edges at {}",
            shown
                .iter()
                .map(|(_, rect)| format!("{:.0}", rect.origin.y.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    shot_of(stage, "vd-toast-4", stage.window());
    hold(stage);
}

/// The middle one dismissed by its own close button, and what the stack does about it.
fn dismissed(stage: &mut Stage<'_>) {
    hold(stage);
    stage.wait(EXPANDED);
    let shown = stack(stage);
    if shown.len() < 2 {
        stage.report.note(
            "Toast",
            &format!("only {} on the screen to dismiss from", shown.len()),
        );
        return;
    }
    let middle = shown[shown.len() / 2];
    // The dismiss control draws an icon and says nothing a census can read, so it is aimed by
    // where the component puts it: a 20px disc hanging just past the toast's top leading corner
    // (`.zui-toast__close` — absolute at the corner, pulled out by a third of itself).
    let closer = Rect::new(
        Point::new(
            DevicePx(middle.1.origin.x.0 - 7.0),
            DevicePx(middle.1.origin.y.0 - 7.0),
        ),
        zgui::geom::Size::new(DevicePx(20.0), DevicePx(20.0)),
    );
    // Preferred over the computed rectangle: the disc's own censused box, which is the box the
    // hit test answers from. The computed corner disc and the censused one disagree in a live
    // window — the disc's hit box sits a dozen pixels from where its layout puts it — and a click
    // that crossed that disagreement would report the queue broken for a fault in the spaces.
    let censused = crate::stage::census::Census::take(stage.handles())
        .nodes
        .iter()
        .filter(|seen| seen.text.is_empty())
        .filter_map(|seen| seen.rect)
        .filter(|rect| rect.size.width.0 <= 30.0 && rect.size.width.0 >= 14.0)
        .filter(|rect| {
            (rect.origin.x.0 - closer.origin.x.0).abs() <= 30.0
                && (rect.origin.y.0 - closer.origin.y.0).abs() <= 30.0
        })
        .min_by(|a, b| {
            (a.size.width.0 * a.size.height.0).total_cmp(&(b.size.width.0 * b.size.height.0))
        })
        .unwrap_or(closer);
    find::mark(stage, "toast:close", censused);
    let others: Vec<NodeId> = shown
        .iter()
        .filter(|(node, _)| *node != middle.0)
        .map(|(node, _)| *node)
        .collect();
    let before = boxes(stage, &others);

    stage.click(Point::new(
        DevicePx(censused.origin.x.0 + censused.size.width.0 / 2.0),
        DevicePx(censused.origin.y.0 + censused.size.height.0 / 2.0),
    ));

    // Frame by frame from here: whether the dismissed one is still boxed, and where the others have
    // got to. A row taken away at once and a row that leaves are the same picture one frame later
    // and different pictures for the twenty in between.
    let mut leaving = 0;
    let mut moved = 0;
    let mut series: Vec<String> = Vec::new();
    for frame in 0..WATCH {
        // Counted through a fresh census rather than through the handle the row had when it was
        // dismissed: dismissing rewrites the row's value, the keyed loop re-renders it, and the
        // box the old handle answers for dies with the old elements while the row itself is still
        // on the screen leaving.
        let gone = stack(stage).len() <= others.len();
        if !gone {
            leaving += 1;
        }
        let now = boxes(stage, &others);
        if now.iter().zip(&before).any(|(now, was)| match (now, was) {
            (Some(now), Some(was)) => (now.origin.y.0 - was.origin.y.0).abs() > 0.5,
            _ => false,
        }) {
            moved += 1;
        }
        series.push(format!(
            "{}{}",
            if gone { "-" } else { "x" },
            now.iter()
                .map(|rect| rect.map_or("_".to_owned(), |rect| format!("{:.0}", rect.origin.y.0)))
                .collect::<Vec<_>>()
                .join("/")
        ));
        if frame == 2 {
            shot_of(stage, "vd-toast-leaving", stage.window());
        }
        // A step of the clock, not merely a frame: the exit is an animation, and an animation
        // sampled at a clock that never moves is a toast that never finishes leaving.
        stage.wait(Duration::from_millis(17));
    }
    let after = boxes(stage, &others);
    stage.report.check(
        "Toast",
        "the close button takes that toast away",
        stack(stage).len() <= others.len(),
        &format!("the dismissed row held a box for {leaving} of {WATCH} frames"),
    );
    stage.report.check(
        "Toast",
        "it leaves over several frames rather than at once",
        leaving > 1 && leaving < WATCH,
        &format!("it was still boxed for {leaving} of {WATCH} frames"),
    );
    stage.report.check(
        "Toast",
        "the ones left over move into the gap",
        moved > 0,
        &format!(
            "they were at {} and are at {} ({moved} of {WATCH} frames differed)",
            before
                .iter()
                .map(|rect| rect.map_or("_".to_owned(), |rect| format!("{:.0}", rect.origin.y.0)))
                .collect::<Vec<_>>()
                .join("/"),
            after
                .iter()
                .map(|rect| rect.map_or("_".to_owned(), |rect| format!("{:.0}", rect.origin.y.0)))
                .collect::<Vec<_>>()
                .join("/")
        ),
    );
    stage
        .report
        .note("Toast", &format!("frame by frame: {}", series.join(" ")));
    shot_of(stage, "vd-toast-closed", stage.window());
}
