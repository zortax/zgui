//! What a stack of toasts puts on the screen, output frame by output frame.
//!
//! # Why these are read from the display list of a real device
//!
//! Three of the five things this component was reported for are claims about frames rather than about
//! states: it *flickers*, the others *do not move down*, and a toast that goes away has *no
//! animation*. Every one of those is invisible to a fixture that settles the window and then looks. A
//! toast that reaches its place, blinks out for one frame and arrives again is at exactly the right
//! place by the time anything has settled; a stack that jumps into a closed gap and one that slides
//! into it are the same photograph a fifth of a second later; and a toast deleted the instant it is
//! dismissed leaves a window that agrees, afterwards, with one that faded it out properly.
//!
//! So the clock is stepped one output frame at a time and every frame is read.

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view::AnyView;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui::toast::{Toast, use_toaster};
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 12px; align-items: flex-start }";

/// How many output frames a toast's entrance or exit takes, with room to spare.
///
/// `--zui-motion-duration-normal` is 180ms, which is eleven frames of a sixty-hertz output.
const MOVE: usize = 16;

/// A page with a toaster and three buttons that announce something.
fn page() -> AnyView {
    AnyView::new(view! {
        ThemeProvider {
            Toaster {
                column(class = "page") {
                    row {
                        Push(title = "one")
                        Push(title = "two")
                        Push(title = "three")
                    }
                }
            }
        }
    })
}

/// A button that announces one message.
#[component]
fn Push(
    /// What the toast says, which is also what the button is called.
    #[prop(into)]
    title: String,
) -> impl IntoView {
    let toasts = use_toaster();
    let label = format!("push {title}");
    view! {
        Button(on:click = move |_| {
            if let Some(toasts) = toasts {
                toasts.push(Toast::new(title.clone()));
            }
        }) {
            {label}
        }
    }
}

/// The corner the stack is in, which is where every reading here is taken.
fn corner() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(400.0), DevicePx(200.0)),
        Size::new(DevicePx(500.0), DevicePx(400.0)),
    )
}

/// The top edge of every toast the most recent frame drew, nearest the corner last.
///
/// A toast is one wide filled rectangle — its background — and its border is another of the same
/// size, so the two are folded together by rounding to the pixel: what is being asked is where the
/// toasts are, and two rectangles a hundredth of a pixel apart are one toast.
///
/// A toast is the only rectangle in the corner as wide as the stack and no taller than a line or two,
/// which is what tells one from the page it is drawn over: the page's own background has its middle
/// inside the corner as well.
fn toasts(stage: &Stage) -> Vec<f32> {
    let mut found: Vec<f32> = stage
        .quads_in(corner())
        .into_iter()
        .filter(|quad| {
            let size = quad.bounds.size;
            (300.0..420.0).contains(&size.width.0) && (24.0..96.0).contains(&size.height.0)
        })
        .map(|quad| quad.bounds.origin.y.0)
        .collect();
    found.sort_by(f32::total_cmp);
    found.dedup_by(|left, right| (*left - *right).abs() < 0.5);
    found
}

/// Opens the page and announces `titles`, oldest first, letting each one arrive.
fn stacked(titles: &[&str]) -> Option<Stage> {
    let mut stage = Stage::open(SHEET, page)?;
    for title in titles {
        let at = stage
            .census()
            .control(&format!("push {title}"))
            .and_then(|node| node.centre())
            .expect("the button is laid out");
        stage.click(at);
        stage.wait(Duration::from_millis(300));
    }
    Some(stage)
}

/// The toast whose title is `title`: the node it is, and where it is.
///
/// A toast says its title and nothing else — its close button is a drawing, and a drawing
/// contributes no characters — so several nested nodes say exactly the title: the text node, the
/// column around it, the toast, and the slot the toast slides inside. The toast is the smallest of
/// them that has a wordless box somewhere under it, which is its close button.
///
/// The slot is the one worth being careful about. It does not move when the toast is pushed, so a
/// fixture that measured it would report a swipe as having gone nowhere.
fn toast_node(stage: &Stage, title: &str) -> Option<(zgui::view::NodeId, Rect<DevicePx, Device>)> {
    let census = stage.census();
    census
        .nodes
        .iter()
        .filter(|node| node.text == title && node.area() > 0.0)
        .filter(|node| under(stage, node.id).any(|seen| seen.text.is_empty() && seen.area() > 0.0))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect.map(|rect| (node.id, rect)))
}

/// The box of the toast whose title is `title`.
fn toast_of(stage: &Stage, title: &str) -> Option<Rect<DevicePx, Device>> {
    toast_node(stage, title).map(|(_, rect)| rect)
}

/// Every laid-out node beneath `node`, itself excluded.
///
/// Beneath, and not *over the same pixels as*. A closed stack draws each toast slightly scaled and
/// offset behind the one in front of it, so the toast at the front has the one behind it
/// overlapping its own box — and a fixture that read "inside" as a question about rectangles would
/// find the buried toast's close button and press a control the stack has put out of reach.
fn under(
    stage: &Stage,
    node: zgui::view::NodeId,
) -> impl Iterator<Item = crate::desktop::census::Seen> + use<> {
    let census = stage.census();
    let host = stage.handles().host.clone();
    census
        .nodes
        .into_iter()
        .filter(move |seen| seen.id != node && host.contains(node, seen.id))
}

/// The middle of the close button on the toast whose title is `title`.
///
/// # Panics
///
/// Panics when that toast is not on the screen, because a fixture that quietly aimed at the origin
/// reports the same thing as a control that does not answer.
fn close_of(stage: &Stage, title: &str) -> Point<DevicePx, Device> {
    let (node, _) = toast_node(stage, title).unwrap_or_else(|| panic!("no toast says {title:?}"));
    under(stage, node)
        .filter(|seen| seen.text.is_empty())
        .filter(|seen| seen.rect.is_some_and(|rect| rect.size.width.0 > 0.0))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|seen| seen.centre())
        .unwrap_or_else(|| panic!("the toast saying {title:?} has no close button laid out"))
}

#[test]
fn a_toast_entering_is_drawn_in_every_frame_and_never_starts_again() {
    // The reported flicker, exactly: the entrance ran, arrived, and then the toast was missing from
    // one frame's display list altogether and set off from its starting place a second time.
    let Some(mut stage) = stacked(&[]) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let at = stage
        .census()
        .control("push one")
        .and_then(|node| node.centre())
        .expect("the button is laid out");
    stage.click(at);

    let mut series = Vec::with_capacity(MOVE * 2);
    for frame in 0..MOVE * 2 {
        stage.tick();
        stage.repaint();
        let drawn = toasts(&stage);
        assert_eq!(
            drawn.len(),
            1,
            "frame {frame} of the entrance drew {} toasts, not one: {series:?}",
            drawn.len()
        );
        series.push(drawn[0]);
    }

    let rest = *series.last().expect("frames were read");
    assert!(
        series[0] - rest > 4.0,
        "the toast moved {}px over its whole entrance, which is not an entrance: {series:?}",
        series[0] - rest
    );
    let arrived = series
        .iter()
        .position(|top| (top - rest).abs() < 0.5)
        .expect("it ends where it ends");
    for (frame, top) in series.iter().enumerate().skip(arrived) {
        assert!(
            (top - rest).abs() < 0.5,
            "frame {frame} left the place the toast had already arrived at: {series:?}"
        );
    }
    for pair in series.windows(2) {
        assert!(
            pair[1] <= pair[0] + 0.01,
            "the toast moved back down on its way in: {series:?}"
        );
    }
    stage.capture("toast-entering");
}

#[test]
fn the_toast_above_a_dismissed_one_slides_into_the_gap() {
    // Not a jump. The place a toast has on the stack is a transform, so the frames between the two
    // places exist and can be counted; when the stack was laid out in flow there was nothing between
    // them at all.
    let Some(mut stage) = stacked(&["one", "two"]) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let before = toasts(&stage);
    assert_eq!(before.len(), 2, "both are on the screen: {before:?}");
    let at = close_of(&stage, "two");
    stage.click(at);

    let mut series = Vec::with_capacity(MOVE);
    for _ in 0..MOVE {
        stage.tick();
        stage.repaint();
        series.push(toasts(&stage).first().copied().unwrap_or(f32::NAN));
    }

    let start = before[0];
    let end = *series.last().expect("frames were read");
    assert!(
        end - start > 8.0,
        "the toast above the dismissed one ended up lower than it started: {start} then {series:?}"
    );
    let between = series
        .iter()
        .filter(|top| **top > start + 0.5 && **top < end - 0.5)
        .count();
    assert!(
        between >= 4,
        "it went from one place to the other in {between} frames in between, which is a jump: \
         {series:?}"
    );
    for pair in series.windows(2) {
        assert!(
            pair[1] >= pair[0] - 0.01,
            "and it wandered back up on the way: {series:?}"
        );
    }
}

#[test]
fn a_dismissed_toast_is_still_drawn_while_it_leaves() {
    // What "no animation when disappearing" was: the row was taken out of the queue in the frame the
    // dismissal was asked for, so there was nothing left for a style sheet to animate.
    let Some(mut stage) = stacked(&["one", "two"]) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let at = close_of(&stage, "two");
    stage.click(at);

    let mut leaving = 0;
    let mut counts = Vec::with_capacity(MOVE * 2);
    for _ in 0..MOVE * 2 {
        stage.tick();
        stage.repaint();
        let drawn = toasts(&stage).len();
        counts.push(drawn);
        if drawn == 2 {
            leaving += 1;
        }
    }
    assert!(
        leaving >= 4,
        "the dismissed toast was drawn on {leaving} frames after it was dismissed: {counts:?}"
    );
    assert_eq!(
        counts.last(),
        Some(&1),
        "and it had gone by the end of them: {counts:?}"
    );
}

#[test]
fn a_toast_pushed_far_enough_goes_and_one_pushed_a_little_stays() {
    let Some(mut stage) = stacked(&["one"]) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let toast = toast_of(&stage, "one").expect("the toast is on the screen");
    let from = Point::new(
        DevicePx(toast.origin.x.0 + 40.0),
        DevicePx(toast.origin.y.0 + toast.size.height.0 / 2.0),
    );

    // A short push is put back, and the toast is still there.
    stage.move_to(from);
    stage.press();
    stage.move_to(Point::new(DevicePx(from.x.0 + 12.0), from.y));
    stage.release();
    stage.wait(Duration::from_millis(300));
    assert_eq!(
        toasts(&stage).len(),
        1,
        "a push of twelve pixels is not a dismissal"
    );

    // A long one takes it away.
    stage.move_to(from);
    stage.press();
    stage.move_to(Point::new(DevicePx(from.x.0 + 90.0), from.y));
    stage.release();
    stage.wait(Duration::from_millis(400));
    assert!(
        toasts(&stage).is_empty(),
        "a push of ninety pixels dismisses it"
    );
}
