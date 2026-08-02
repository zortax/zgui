//! The two questions the page is asked after every single cycle.
//!
//! A window that has been deafened by a surface which did not clean up after itself still hovers,
//! still scrolls and still paints. What it does not do is act on a press or on a key, because both
//! of those are routed through the things a modal installs and a teardown is supposed to remove.
//! So the questions are exactly those two, and each is answered from the compositor's own pixels
//! as well as from the document.
//!
//! Both are asked with the pointer already parked where it is going to press, and the two pictures
//! taken from that one position. A picture of a control at rest and a picture of the same control
//! with the pointer on it differ whatever the press did, and a comparison between them would be
//! satisfied by a window that had done nothing but notice the pointer.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::vocab::{Modifiers, NamedKey, PointerButton};

use crate::script::find;
use crate::script::gauntlet::ink;
use crate::stage::Stage;

/// The panel whose tab strip is pressed.
const TABS: &str = "Tabs";

/// The tab that moves the strip off what it is showing.
const AWAY: &str = "Billing";

/// What that tab shows.
const AWAY_TEXT: &str = "Cards, invoices and the plan you are on.";

/// The tab that puts it back.
const BACK: &str = "Profile";

/// What that one shows.
const BACK_TEXT: &str = "Your name, your picture and how to reach you.";

/// The panel holding the field that is typed into.
const FIELDS: &str = "Input and textarea";

/// The label above that field.
const LABEL: &str = "Display name";

/// What is typed into it, and then taken back out again.
const TYPED: &str = "z";

/// How far below the label's box the field is pressed, in device pixels.
const BELOW: f32 = 20.0;

/// How much wider than the field the picture of it is taken, so that its focus ring is inside.
const MARGIN: f32 = 10.0;

/// What the page did when it was asked.
#[derive(Copy, Clone)]
pub(crate) struct Answer {
    /// Whether a press on an ordinary control still does what it says.
    pub(crate) pointer: bool,
    /// Whether a key still reaches the caret.
    pub(crate) key: bool,
}

/// Presses an ordinary control and types a character, capturing both, under names from `tag`.
pub(crate) fn press_and_type(stage: &mut Stage<'_>, tag: &str) -> Answer {
    Answer {
        pointer: tabs(stage, tag),
        key: typing(stage, tag),
    }
}

/// Moves the page's own tab strip to another tab and back again.
///
/// A control far from every overlay whose answer is a change in the layout — one panel of prose
/// goes and another arrives — rather than a click into space, which nothing could contradict. It
/// is pressed twice so that the page the next cycle starts from is the page this one started
/// from.
fn tabs(stage: &mut Stage<'_>, tag: &str) -> bool {
    let Some((census, panel)) = find::open_panel(stage, TABS) else {
        return false;
    };
    let (Some(away), Some(back)) = (
        find::at_in(&census, panel, AWAY),
        find::at_in(&census, panel, BACK),
    ) else {
        return false;
    };
    stage.move_to(away);
    stage.settle(4);
    ink::shot_of(stage, &format!("gx-{tag}-t0"), panel);
    stage.press_release(PointerButton::Primary, Modifiers::NONE);
    stage.settle(8);
    let moved = says(stage, AWAY_TEXT) && !says(stage, BACK_TEXT);
    ink::shot_of(stage, &format!("gx-{tag}-t1"), panel);
    stage.click(back);
    stage.settle(8);
    let returned = says(stage, BACK_TEXT) && !says(stage, AWAY_TEXT);
    moved && returned
}

/// Puts the caret in a text field, types one character and takes it back out.
///
/// Which node the character went to is not assumed: whatever the press left focused is what is
/// measured and what is photographed. A window whose keyboard is still confined to a surface that
/// closed minutes ago focuses nothing here, and says so.
fn typing(stage: &mut Stage<'_>, tag: &str) -> bool {
    let Some((census, panel)) = find::open_panel(stage, FIELDS) else {
        return false;
    };
    let Some(label) = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text == LABEL && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
    else {
        return false;
    };
    let into = Point::new(
        DevicePx(label.origin.x.0 + label.size.width.0 / 2.0),
        DevicePx(label.origin.y.0 + label.size.height.0 + BELOW),
    );
    stage.click(into);
    stage.settle(6);
    let Some(field) = stage.focused() else {
        return false;
    };
    let Some(rect) = stage.census().node(field).and_then(|node| node.rect) else {
        return false;
    };
    let before = held(stage);
    ink::shot_of(stage, &format!("gx-{tag}-k0"), grown(rect));
    stage.type_text(TYPED);
    stage.settle(6);
    let typed = held(stage);
    ink::shot_of(stage, &format!("gx-{tag}-k1"), grown(rect));
    stage.key(NamedKey::Backspace);
    stage.settle(6);
    let undone = held(stage);
    ink::shot_of(stage, &format!("gx-{tag}-k2"), grown(rect));
    typed != before && undone == before
}

/// Everything the field's panel says, which is where a typed character turns up.
fn held(stage: &Stage<'_>) -> String {
    stage
        .census()
        .panel(FIELDS)
        .map(|node| node.text.clone())
        .unwrap_or_default()
}

/// Whether `text` is laid out with a box of its own.
fn says(stage: &Stage<'_>, text: &str) -> bool {
    stage
        .census()
        .nodes
        .iter()
        .any(|node| node.text == text && node.area() > 0.0)
}

/// `rect` with a margin around it, so that what is drawn just outside a control is in the picture.
fn grown(rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(
            DevicePx(rect.origin.x.0 - MARGIN),
            DevicePx(rect.origin.y.0 - MARGIN),
        ),
        zgui::geom::Size::new(
            DevicePx(rect.size.width.0 + MARGIN * 2.0),
            DevicePx(rect.size.height.0 + MARGIN * 2.0),
        ),
    )
}
