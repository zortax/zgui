//! What a stack of toasts does: where each one lands, when it goes, and what takes it away.
//!
//! Every reading here is taken from the laid-out document of a real window — where the toast is, and
//! whether it is there at all — because each of the five things this component was reported for is a
//! claim about the screen. A queue whose rows are in the right order and whose toasts are drawn in
//! the wrong place is a queue that passes every test of itself.

mod desktop;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui::toast::{Toast, ToastCorner, use_toaster};
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::{HEIGHT, Stage};

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 40px; gap: 12px; align-items: flex-start }
                     .low { padding-top: 400px }";

/// A page with a toaster and one button per message it can announce.
///
/// Inside a `ThemeProvider`, because that is what an application does and what decides the space
/// between two toasts: a stack whose gap resolved to nothing would pass an assertion about toasts
/// clearing each other by touching.
#[component]
fn Page() -> impl IntoView {
    view! {
        ThemeProvider {
            Toaster {
                column(class = "page") {
                    row {
                        Push(title = "one")
                        Push(title = "two")
                        Push(title = "three")
                        Push(title = "four")
                    }
                    row {
                        Push(title = "brief", seconds = 1)
                        Push(title = "patient", seconds = 8)
                    }
                }
            }
        }
    }
}

/// The same page with the stack in the opposite corner.
#[component]
fn UpsideDown() -> impl IntoView {
    view! {
        ThemeProvider {
            Toaster(corner = ToastCorner::TopLeft) {
                column(class = "page low") {
                    row {
                        Push(title = "one")
                        Push(title = "two")
                    }
                }
            }
        }
    }
}

/// A button that announces one message, with a deadline of its own when one is given.
#[component]
fn Push(
    /// What the toast says, which is also what the button is called.
    #[prop(into)]
    title: String,
    /// How many seconds it stays, or the usual deadline when nothing says.
    #[prop(optional)]
    seconds: Option<u64>,
) -> impl IntoView {
    let toasts = use_toaster();
    let label = format!("push {title}");
    view! {
        Button(on:click = move |_| {
            if let Some(toasts) = toasts {
                let mut toast = Toast::new(title.clone());
                if let Some(seconds) = seconds {
                    toast = toast.duration(Duration::from_secs(seconds));
                }
                toasts.push(toast);
            }
        }) {
            {label}
        }
    }
}

/// The toast whose title is `title`: the node it is, and where it is.
///
/// A toast says its title and nothing else — its close button is a drawing, and a drawing
/// contributes no characters — so several nested nodes say exactly the title: the text node, the
/// column around it, the toast, and the slot the toast slides inside. What separates the toast from
/// the two within it is that it *holds the close button*, and what separates it from the slot
/// outside it is that it is the smaller of the two. So: the smallest node saying the title that has
/// a wordless box somewhere under it.
///
/// The slot is the one worth being careful about. It is taller by the gap it carries and does not
/// move when the toast is pushed, so a fixture that measured it would report a swipe as having gone
/// nowhere and a stack as having no space between its rows.
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
/// Beneath, and not *over the same pixels as*. A collapsed stack draws each toast slightly scaled
/// and offset behind the one in front of it, so the toast at the front of the stack has the one
/// behind it overlapping its own box — and a fixture that read "inside" as a question about
/// rectangles would find the buried toast's close button, aim at it, and press a control that the
/// stack has deliberately put out of reach. Which node is *part of* which toast is a question about
/// the tree, so it is asked of the tree.
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

/// How far down the window the bottom of a rectangle is.
fn bottom(rect: Rect<DevicePx, Device>) -> f32 {
    rect.origin.y.0 + rect.size.height.0
}

/// A window with `titles` announced in order, oldest first, and everything settled.
fn stacked(titles: &[&str]) -> Stage {
    let mut stage = Stage::open(SHEET, || view! { Page() });
    for title in titles {
        stage.click_saying(&format!("push {title}"));
        stage.hold(Duration::from_millis(300));
    }
    stage
}

#[test]
fn the_newest_toast_is_the_one_against_the_corner() {
    // The defect: the stack grew the other way, so the toast that had just been announced was the
    // one furthest from the corner and every earlier one was shifted towards it. Which corner the
    // stack is anchored to is the whole of what a corner means.
    let stage = stacked(&["one", "two", "three"]);
    let one = toast_of(&stage, "one").expect("the first is on the screen");
    let two = toast_of(&stage, "two").expect("the second is on the screen");
    let three = toast_of(&stage, "three").expect("the third is on the screen");

    assert!(
        bottom(three) > bottom(two) && bottom(two) > bottom(one),
        "the newest is against the bottom corner and the older ones are above it: \
         one {one:?}, two {two:?}, three {three:?}"
    );
    assert!(
        bottom(three) > HEIGHT - 64.0 && bottom(three) < HEIGHT,
        "and the newest is at the window's own bottom edge, not somewhere in the middle: {three:?}"
    );
}

#[test]
fn no_two_toasts_are_drawn_over_each_other_once_the_stack_is_open() {
    // Measured rather than stepped: a toast with a description is taller than one without, and a
    // stack stepped by one fixed amount puts one toast through another the first time a caller
    // writes a longer message.
    //
    // Asked of the *open* stack, and that is the whole of what makes it a claim. A closed stack is
    // a deck — each message sits a little behind and a little smaller than the one in front, and
    // only the front one says anything — so its rows overlap on purpose and always will. The step
    // between them is only ever a measurement once the stack has opened under the pointer, which is
    // also the only moment all three are there to be read.
    let mut stage = stacked(&["one", "two", "three"]);
    let front = toast_of(&stage, "three").expect("the newest is on the screen");
    stage.move_to(Point::new(
        DevicePx(front.origin.x.0 + 8.0),
        DevicePx(front.origin.y.0 + front.size.height.0 / 2.0),
    ));
    stage.hold(Duration::from_millis(1200));

    let mut boxes: Vec<Rect<DevicePx, Device>> = ["one", "two", "three"]
        .iter()
        .map(|title| toast_of(&stage, title).expect("it is on the screen"))
        .collect();
    boxes.sort_by(|left, right| left.origin.y.0.total_cmp(&right.origin.y.0));
    for pair in boxes.windows(2) {
        let gap = pair[1].origin.y.0 - bottom(pair[0]);
        assert!(
            gap > 0.0,
            "these two overlap by {}px: {:?} and {:?}",
            -gap,
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn the_close_button_takes_away_the_toast_it_belongs_to() {
    // The defect this pins: the toast captured the pointer on the press, so the release that its own
    // close button was waiting for was delivered to the toast instead and the click never happened.
    // The middle one is chosen because a defect that dismissed the newest, or all of them, passes a
    // test that only ever closes one.
    let mut stage = stacked(&["one", "two", "three"]);
    let at = close_of(&stage, "two");
    stage.click(at);
    stage.hold(Duration::from_millis(400));

    assert!(
        toast_of(&stage, "two").is_none(),
        "the toast whose close button was pressed is gone"
    );
    assert!(
        toast_of(&stage, "one").is_some() && toast_of(&stage, "three").is_some(),
        "and the other two are still there"
    );
}

#[test]
fn the_toasts_above_a_dismissed_one_close_the_gap() {
    // The newest is the one against the corner, so dismissing it is what leaves a hole in the stack.
    let mut stage = stacked(&["one", "two", "three"]);
    let corner = toast_of(&stage, "three").expect("the newest is on the screen");
    let was = toast_of(&stage, "two").expect("the second is on the screen");
    let at = close_of(&stage, "three");
    stage.click(at);
    stage.hold(Duration::from_millis(400));

    let now = toast_of(&stage, "two").expect("the second is still on the screen");
    assert!(
        bottom(now) > bottom(was) + 8.0,
        "the toast above the dismissed one moved down into the gap: {was:?} then {now:?}"
    );
    assert!(
        (bottom(now) - bottom(corner)).abs() < 2.0,
        "and it moved into exactly the place the dismissed one had: {corner:?} then {now:?}"
    );
    let above = toast_of(&stage, "one").expect("the oldest is still on the screen");
    assert!(
        bottom(above) < bottom(now),
        "with the older one still above it: {above:?} and {now:?}"
    );
}

#[test]
fn a_fourth_toast_pushes_the_oldest_out_instead_of_deleting_it() {
    // Three may show. What the fourth does to the first is the reported defect: it was truncated out
    // of the queue in the same frame, so it had no exit and everything above it moved at once.
    let mut stage = stacked(&["one", "two", "three"]);
    stage.click_saying("push four");
    stage.settle();

    assert!(
        toast_of(&stage, "one").is_some(),
        "the oldest is still on the screen, on its way out"
    );
    stage.hold(Duration::from_millis(400));
    assert!(
        toast_of(&stage, "one").is_none(),
        "and it has gone by the time its exit has finished"
    );
    for title in ["two", "three", "four"] {
        assert!(
            toast_of(&stage, title).is_some(),
            "{title} is one of the three that stay"
        );
    }
}

#[test]
fn each_toast_expires_on_its_own_deadline() {
    let mut stage = Stage::open(SHEET, || view! { Page() });
    stage.click_saying("push patient");
    stage.click_saying("push brief");
    stage.hold(Duration::from_millis(1_600));

    assert!(
        toast_of(&stage, "brief").is_none(),
        "the one-second toast has gone"
    );
    assert!(
        toast_of(&stage, "patient").is_some(),
        "and the eight-second one is still being read"
    );
}

#[test]
fn the_whole_stack_waits_while_the_pointer_is_on_any_of_it() {
    // Any of it, not merely the toast under the pointer: reading the second of three messages must
    // not let the first and third disappear from under it.
    let mut stage = stacked(&["one", "two", "three"]);
    let middle = toast_of(&stage, "two").expect("the second is on the screen");
    stage.move_to(Point::new(
        DevicePx(middle.origin.x.0 + 8.0),
        DevicePx(middle.origin.y.0 + middle.size.height.0 / 2.0),
    ));
    stage.hold(Duration::from_secs(6));
    for title in ["one", "two", "three"] {
        assert!(
            toast_of(&stage, title).is_some(),
            "{title} waited while the pointer was on the stack"
        );
    }

    stage.move_to(Point::new(DevicePx(40.0), DevicePx(40.0)));
    stage.hold(Duration::from_secs(5));
    for title in ["one", "two", "three"] {
        assert!(
            toast_of(&stage, title).is_none(),
            "{title} expired once the pointer had left"
        );
    }
}

#[test]
fn a_toast_dismissed_from_under_the_pointer_lets_the_stack_go() {
    // The pointer never leaves it: the toast is taken away while the pointer is still inside it, so
    // no leave event is ever delivered. A stack that counted that press and never had it given back
    // would hold every deadline after it for the rest of the window's life.
    let mut stage = stacked(&["one"]);
    let at = close_of(&stage, "one");
    stage.click(at);
    stage.hold(Duration::from_millis(400));
    assert!(toast_of(&stage, "one").is_none(), "it went");

    stage.click_saying("push two");
    stage.hold(Duration::from_secs(5));
    assert!(
        toast_of(&stage, "two").is_none(),
        "and the next toast still expires on its own deadline"
    );
}

#[test]
fn a_stack_in_the_opposite_corner_grows_the_other_way_and_still_lets_go() {
    // Both halves in one place. Which way the stack grows is the corner's own business; that the
    // toast leaves *promptly* is the exit's, and an exit whose end nothing recognises is retired a
    // second later by a deadline rather than when it finishes — which is what this timing catches.
    let mut stage = Stage::open(SHEET, || view! { UpsideDown() });
    stage.click_saying("push one");
    stage.hold(Duration::from_millis(300));
    stage.click_saying("push two");
    stage.hold(Duration::from_millis(300));

    let one = toast_of(&stage, "one").expect("the first is on the screen");
    let two = toast_of(&stage, "two").expect("the second is on the screen");
    assert!(
        two.origin.y.0 < one.origin.y.0,
        "the newest is against the top corner and the older one is below it: {one:?}, {two:?}"
    );
    assert!(
        two.origin.y.0 < 64.0 && two.origin.x.0 < 64.0,
        "and the stack is in the top leading corner: {two:?}"
    );

    let at = close_of(&stage, "two");
    stage.click(at);
    stage.hold(Duration::from_millis(400));
    assert!(
        toast_of(&stage, "two").is_none(),
        "the dismissed toast goes when its exit has finished, not a second later"
    );
    let now = toast_of(&stage, "one").expect("the other one is still there");
    assert!(
        (now.origin.y.0 - two.origin.y.0).abs() < 2.0,
        "and the one below it has closed the gap: {two:?} then {now:?}"
    );
}

#[test]
fn a_toast_and_the_control_that_dismisses_it_are_both_announced() {
    let stage = stacked(&["one"]);
    let announced = stage.announced();
    let said = |role: &str, name: &str| {
        announced
            .iter()
            .any(|node| node.role == role && node.name == name)
    };
    assert!(
        announced.iter().any(|node| node.role == "Status"),
        "the stack is a region a reader can go back to: {announced:?}"
    );
    assert!(
        said("Alert", "one"),
        "the toast itself is announced: {announced:?}"
    );
    assert!(
        said("Button", "Dismiss"),
        "and the control that takes it away has a name: {announced:?}"
    );
}
