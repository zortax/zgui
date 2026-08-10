//! Where a modal surface's panel actually lands, in a window that has a theme in it.
//!
//! A dialog is centred by two halves that live in different sheets: `left: 50%; top: 50%` puts its
//! leading corner at the middle of the window, and a transform pulls it back by half of itself.
//! The pull-back is a custom property the dialog sets and the shared surface rule composes with the
//! motion every surface enters and leaves by — so it passes through the cascade, a substitution and
//! an interpolation before it reaches a pixel, and it is invisible in the document at every step.
//! Nothing in the tree changes when it is lost: the box is mounted, placed, sized and reported at
//! the window's middle, which is exactly where a panel that has lost its pull-back is *supposed*
//! to be. Only the picture disagrees, by half a panel down and to the right.
//!
//! # Why the fixture has a theme in it
//!
//! Every dialog in the gallery is inside a [`ThemeProvider`], and it is the arrangement the defect
//! was reported from. It is also the one where a lost transform is hardest to notice from the
//! outside: with the tokens in force the panel is opaque, padded and shadowed, so it looks like a
//! finished dialog wherever it lands.
//!
//! # Why the settled reading is pixels and the moving one is not
//!
//! Once the entrance is over the panel is opaque, so the colour it is painted in is the colour on
//! the screen and the bounding box of that colour is where the panel is — a reading that owes
//! nothing to this framework's own idea of where it put things. *During* the entrance the surface
//! is part way through fading in and every pixel of it is a blend with what is behind it, which no
//! colour match can find; there the rectangle the frame carried is the reading, and it is the same
//! rectangle, because the display list records where the ink lands rather than where the box was
//! laid out.

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::view;
use zgui::vocab::NamedKey;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{HEIGHT, SETTLED, Stage, WIDTH};
use crate::painted::words::{aim, assert_absent, assert_painted};

/// The page the fixture is laid out on.
///
/// The panel is given a colour of its own, and that is the whole of the pixel reading: the tokens
/// paint every surface in this library the same near-white, which is also what the page behind it
/// is, and a bounding box of "the colour of a dialog" would be a bounding box of the window. An
/// element-and-class selector out-specifies the token rule without touching anything else about
/// the surface, and magenta is a colour no token in the library produces.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     box.zui-dialog { background-color: #ff00ff; color: #ffffff }";

/// The same page, with the surface's *transform* slowed right down and its opacity left in the
/// tokens' own hurry.
///
/// This is what makes an entrance readable at all. At the library's own timing the two finish
/// together, so every frame in which the panel is solid enough to find is a frame in which it has
/// already almost stopped moving; slowing one and not the other separates them.
const SLOW: &str = "box.zui-dialog {
                        transition: opacity 60ms linear, transform 1200ms linear;
                    }";

/// How far a pixel's red *and* blue have to be above its green before it is the panel.
///
/// The panel is painted magenta, and the test is a shape rather than a colour because the surface
/// fades in: every pixel of it is a blend with the page and the scrim behind it until the entrance
/// is over, and an exact match would find the panel only once it had stopped moving — which is the
/// half of this that needs no fixture. A blend keeps the shape. Nothing else in the window has it:
/// the page is white and the scrim neutral, and both of those separate no channels at all.
const MAGENTA: i32 = 50;

/// How many matching pixels have to sit side by side before they are the panel rather than an edge.
///
/// A separated channel is not quite unique to the panel: the trigger is a solid dark button with
/// near-white text on it, and the renderer's antialiasing leaves a coloured fringe a pixel or two
/// wide along a glyph where the two meet. One such pixel three hundred rows above the panel drags
/// the bounding box a quarter of the window and turns this fixture into a fixture about text.
///
/// A panel is four hundred and seventy-eight pixels wide, so any row of it that is on the screen at
/// all is a run far longer than this; a fringe never is.
const RUN: usize = 8;

/// How far the panel's middle may be from the window's before it looks off-centre.
///
/// One pixel each way, for a panel whose height is odd and a middle that therefore falls between
/// two rows. The defect this measures is a quarter of the window.
const SLACK: f32 = 1.5;

/// How far the entrance may carry the middle while it is still running.
///
/// The shared entrance is `translateY(-2px) scale(0.98)` about the panel's own centre: the scale
/// leaves the middle where it is and the translation lifts it by at most two pixels. Anything
/// beyond that is the placement moving, which is what this fixture exists to catch.
const MOVING: f32 = 2.5;

/// One output frame, which is how far the clock is moved between readings of the entrance.
const FRAME: Duration = Duration::from_micros(16_667);

/// How many frames of the entrance are read.
const FRAMES: usize = 24;

/// What the dialog's trigger says.
const TRIGGER: &str = "Open dialog";

/// What the dialog says, which is how the fixture knows it is on the screen at all.
const TITLE: &str = "Rename project";

/// Opens the fixture, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($sheet:expr) => {
        match Stage::open($sheet, || {
            view! {
                ThemeProvider {
                    column(class = "page") {
                        Dialog {
                            DialogTrigger {{TRIGGER}}
                            DialogContent {
                                DialogTitle {{TITLE}}
                            }
                        }
                    }
                }
            }
        }) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// The whole surface.
fn window() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
    )
}

/// The middle of a rectangle.
fn middle(rect: Rect<DevicePx, Device>) -> (f32, f32) {
    (
        rect.origin.x.0 + rect.size.width.0 / 2.0,
        rect.origin.y.0 + rect.size.height.0 / 2.0,
    )
}

/// Where the panel's colour is on the screen, as the rectangle containing every pixel of it, or
/// nothing when none of it has been drawn yet.
///
/// See [`MAGENTA`] for what counts as the panel's colour and why it is not an exact match.
fn painted_panel(stage: &Stage) -> Option<Rect<DevicePx, Device>> {
    let colours = stage.colours_in(window());
    let width = WIDTH as usize;
    let mut left = usize::MAX;
    let mut top = usize::MAX;
    let mut right = 0usize;
    let mut bottom = 0usize;
    let panel = |colour: &(u8, u8, u8)| {
        let green = i32::from(colour.1);
        i32::from(colour.0) - green >= MAGENTA && i32::from(colour.2) - green >= MAGENTA
    };
    for (y, row) in colours.chunks(width).enumerate() {
        let mut x = 0usize;
        while x < row.len() {
            if !panel(&row[x]) {
                x += 1;
                continue;
            }
            let start = x;
            while x < row.len() && panel(&row[x]) {
                x += 1;
            }
            if x - start < RUN {
                continue;
            }
            left = left.min(start);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y + 1);
        }
    }
    (left != usize::MAX).then(|| {
        Rect::new(
            Point::new(DevicePx(left as f32), DevicePx(top as f32)),
            Size::new(
                DevicePx((right - left) as f32),
                DevicePx((bottom - top) as f32),
            ),
        )
    })
}

/// The panel, which has to be somewhere.
///
/// # Panics
///
/// Panics when none of its colour is on the screen, because a fixture that measured an empty
/// region would report the same thing for a panel drawn off the window as for one drawn in the
/// middle of it.
fn panel(stage: &Stage) -> Rect<DevicePx, Device> {
    painted_panel(stage).expect("the panel's colour is nowhere on the screen")
}

/// Asserts that `panel` is centred on the window, within `slack`.
fn assert_centred(panel: Rect<DevicePx, Device>, slack: f32, what: &str) {
    let (want_x, want_y) = middle(window());
    let (got_x, got_y) = middle(panel);
    assert!(
        (got_x - want_x).abs() <= slack && (got_y - want_y).abs() <= slack,
        "{what}: the panel's middle is ({got_x}, {got_y}) and the window's is ({want_x}, \
         {want_y}) — it is at {panel:?}"
    );
}

/// Opens the dialog and settles it.
fn open(stage: &mut Stage) {
    let at = aim(stage, TRIGGER);
    stage.click(at);
    stage.wait(SETTLED);
    assert_painted(stage, TITLE);
}

/// Where the window says the panel is.
///
/// The largest box that says what the dialog says and is smaller than the window: the surface is
/// the outermost thing whose whole text is the title, and the overlay band it hangs off — which
/// says the same and is the size of the window — is the one thing above it.
///
/// # Panics
///
/// Panics when nothing on the overlay band is the surface.
fn reported(stage: &Stage) -> Rect<DevicePx, Device> {
    let census = stage.census();
    let whole = WIDTH * HEIGHT;
    let node = census
        .nodes
        .iter()
        .filter(|node| node.text == TITLE && node.area() > 0.0 && node.area() < whole * 0.9)
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .expect("the dialog is in the document");
    node.rect.expect("a node with area has a box")
}

#[test]
fn a_dialog_inside_a_theme_provider_is_painted_in_the_middle_of_the_window() {
    let mut stage = staged!(SHEET);
    open(&mut stage);

    let panel = panel(&stage);
    assert_centred(panel, SLACK, "settled");

    // And the window agrees with the picture. What the framework answers about where the panel is
    // is what a surface placed against something on it is told, so a picture that is centred and
    // an answer that is not is a menu opened from inside this dialog appearing half a panel away
    // from the control that opened it.
    let box_ = reported(&stage);
    assert!(
        (box_.origin.x.0 - panel.origin.x.0).abs() <= SLACK
            && (box_.origin.y.0 - panel.origin.y.0).abs() <= SLACK
            && (box_.size.width.0 - panel.size.width.0).abs() <= 2.0
            && (box_.size.height.0 - panel.size.height.0).abs() <= 2.0,
        "the panel is painted at {panel:?} and the window says it is at {box_:?}"
    );
    stage.capture("dialog-centred-under-a-theme");
}

#[test]
fn the_placement_survives_the_motion_it_is_composed_with() {
    // The place and the motion are two custom properties composed into one `transform`, and the
    // motion is the half that moves. A surface that kept the motion and lost the place is centred in
    // no frame at all; one that kept both is centred in every frame of its own animation. So the
    // middle is read while the surface is leaving, frame by frame, and the animation is required to
    // have actually moved something — otherwise this measures a stationary panel twenty times over
    // and calls it a transition.
    let mut stage = staged!(&format!("{SHEET}\n{SLOW}"));
    open(&mut stage);
    assert_centred(panel(&stage), SLACK, "at rest");

    stage.press_named(NamedKey::Escape);
    // One output frame at a time, each ending in a picture of the whole window, so what is read is
    // a moment of the animation rather than the end of it. Frames in which the surface has faded
    // past finding are skipped rather than asserted about: there is no panel in them to measure.
    let mut seen: Vec<f32> = Vec::new();
    for _ in 0..FRAMES {
        stage.wait(FRAME);
        let Some(panel) = painted_panel(&stage) else {
            continue;
        };
        assert_centred(panel, MOVING, "part way through leaving");
        seen.push(panel.size.width.0);
    }
    assert!(
        seen.len() >= 3,
        "the animation was read in {} frames, which is too few to be about a surface moving",
        seen.len()
    );
    let smallest = seen.iter().copied().fold(f32::INFINITY, f32::min);
    let largest = seen.iter().copied().fold(0.0f32, f32::max);
    assert!(
        largest - smallest > 0.5,
        "the panel was the same size in every frame of its animation ({seen:?}), so nothing was \
         interpolating and the readings above are not about a moving surface"
    );

    stage.wait(Duration::from_millis(1_600));
    assert_absent(&stage, TITLE);
}

#[test]
fn a_dialog_opened_again_is_centred_again() {
    // The closed state declares a motion of its own, and the surface is rebuilt every time it
    // opens. A placement that only reached the first build, or that the closed state's declaration
    // displaced, is centred once and never again.
    let mut stage = staged!(SHEET);
    for round in 1..=3 {
        open(&mut stage);
        assert_centred(panel(&stage), SLACK, &format!("opening {round}"));
        stage.press_named(NamedKey::Escape);
        stage.wait(SETTLED);
        assert_absent(&stage, TITLE);
    }
}
