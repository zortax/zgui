//! Whether a dialog's content is drawn in *both* halves of its life: while the enter animation is
//! still running, and after it has settled.
//!
//! The report this reproduces had three symptoms with one shape. Some of the words — a Cancel
//! button's label, a field's placeholder — appeared only once the enter animation finished; the
//! vector drawings — a select's chevron, an alert's triangle — did the opposite, there while the
//! animation ran and gone the frame it settled; and the scrim vanished for good when the window was
//! resized under an open dialog. What was under them: while the surface's transform is animating,
//! its text is promoted to outline glyphs, and the vector pipeline resolved residual clips and the
//! draw order in a different coordinate space from every other primitive
//! (`zgui-render-vector-vello`'s residual placement, and [`VectorItem::local_ink`]); and an element
//! whose finished animation the engine dropped on an unrelated recascade painted from the first
//! keyframe that cascade had captured, because nothing re-settled its style
//! ([`Animator::tick`]'s cascade for retired holds).
//!
//! [`VectorItem::local_ink`]: zgui_scene::VectorItem
//! [`Animator::tick`]: zgui_anim::Animator::tick

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::view;
use zgui::view::AnyView;
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::status::ALERT_TRIANGLE;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{HEIGHT, SETTLED, Stage, WIDTH};
use crate::painted::words::{aim, ink};

/// The page the dialog opens over.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// Half of the dialog's 200ms entrance: a frame the animation is unmistakably mid-flight in.
const MID_FLIGHT: Duration = Duration::from_millis(100);

/// Opens the fixture, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// The rename dialog the report names: a title, a described field with a placeholder, and the two
/// footer buttons.
fn rename_dialog() -> AnyView {
    AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Dialog {
                    DialogTrigger {"Open dialog"}
                    DialogContent {
                        DialogTitle {"Rename project"}
                        DialogDescription {"Give it a name the invoices can carry."}
                        Input(placeholder = "Project name", label = "Project name")
                        DialogFooter {
                            DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
                            Button {"Rename"}
                        }
                    }
                }
            }
        }
    })
}

/// The delete dialog the report names: the alert with the triangle drawn above its words.
fn delete_dialog() -> AnyView {
    AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                AlertDialog {
                    AlertDialogTrigger(variant = ButtonVariant::Destructive) {"Delete"}
                    AlertDialogContent(size = AlertDialogSize::Sm) {
                        AlertDialogMedia {Icon(icon = ALERT_TRIANGLE, label = "")}
                        AlertDialogHeader {
                            AlertDialogTitle {"Delete this project?"}
                            AlertDialogDescription {
                                "Its history goes with it, and none of it can be recovered."
                            }
                        }
                        AlertDialogFooter {
                            AlertDialogCancel {"Keep it"}
                            AlertDialogAction(variant = ButtonVariant::Destructive) {"Delete"}
                        }
                    }
                }
            }
        }
    })
}

/// Whether `text` has pixels in its box, read off the accumulated picture.
#[track_caller]
fn assert_inked(stage: &Stage, text: &str, moment: &str) {
    let census = stage.census();
    let rect = census
        .nodes
        .iter()
        .filter(|node| node.text == text)
        .filter_map(|node| node.rect)
        .filter(|rect| rect.size.width.0 > 0.0 && rect.size.height.0 > 0.0)
        .min_by(|a, b| {
            (a.size.width.0 * a.size.height.0).total_cmp(&(b.size.width.0 * b.size.height.0))
        })
        .unwrap_or_else(|| panic!("nothing placed says {text:?}"));
    let coverage = ink(stage, rect);
    assert!(
        coverage > 0.01,
        "{moment}: {text:?} has a box at {rect:?} and no pixels in it: ink {coverage}"
    );
}

/// Where the alert's triangle is: the smallest box of the media band's line.
fn icon_rect(stage: &Stage) -> zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device> {
    let census = stage.census();
    census
        .nodes
        .iter()
        .filter(|node| {
            node.rect.is_some_and(|rect| {
                rect.size.width.0 > 4.0
                    && rect.size.width.0 <= 64.0
                    && rect.size.height.0 > 4.0
                    && rect.size.height.0 <= 64.0
            })
        })
        .filter(|node| node.text.is_empty())
        .filter_map(|node| node.rect)
        .find(|rect| {
            // The icon is the empty-texted small box above the title.
            let title = census
                .nodes
                .iter()
                .find(|node| node.text == "Delete this project?")
                .and_then(|node| node.rect);
            title.is_some_and(|title| rect.origin.y.0 < title.origin.y.0)
        })
        .expect("the alert's media band holds a drawn box above the title")
}

/// The words of the rename dialog are inked while the entrance is still running.
#[test]
fn the_words_are_inked_while_the_dialog_is_arriving() {
    let mut stage = staged!(rename_dialog);
    stage.wait(SETTLED);
    let at = aim(&stage, "Open dialog");
    stage.click(at);
    // Step to mid-flight without a repaint: the claim is about the animated frames themselves.
    stage.wait_quietly(MID_FLIGHT);
    stage.capture("dialog-mid-flight");
    // The placeholder is not a census text node, so the words stand in for it; the placeholder is
    // asserted through the gallery probe instead.
    for words in ["Rename project", "Cancel", "Rename"] {
        assert_inked(&stage, words, "mid-flight");
    }
}

/// And they are still inked once it has settled, which the earlier suite already holds.
#[test]
fn the_words_are_inked_after_the_dialog_settles() {
    let mut stage = staged!(rename_dialog);
    stage.wait(SETTLED);
    let at = aim(&stage, "Open dialog");
    stage.click(at);
    stage.wait(SETTLED);
    for words in ["Rename project", "Cancel", "Rename"] {
        assert_inked(&stage, words, "settled");
    }
}

/// The alert's triangle is drawn while the entrance runs, and still drawn after it has settled —
/// without a full repaint in between, because a full repaint repairs exactly the defect reported.
#[test]
fn the_triangle_survives_the_dialog_settling() {
    let mut stage = staged!(delete_dialog);
    stage.wait(SETTLED);
    let at = aim(&stage, "Delete");
    stage.click(at);
    stage.wait_quietly(MID_FLIGHT);
    stage.capture("alert-mid-flight");
    let mid = ink(&stage, icon_rect(&stage));
    assert!(mid > 0.01, "mid-flight: the triangle left no pixels: {mid}");

    stage.wait_quietly(SETTLED);
    stage.capture("alert-settled");
    let after = ink(&stage, icon_rect(&stage));
    assert!(
        after > 0.01,
        "settled: the triangle left no pixels: {after}"
    );

    // And a complete picture agrees: the settled frame carries the drawing.
    stage.repaint();
    let repainted = ink(&stage, icon_rect(&stage));
    assert!(
        repainted > 0.01,
        "a full repaint of the settled dialog has no triangle: {repainted}"
    );
}

/// The scrim keeps dimming the page when the window is resized under an open dialog.
#[test]
fn the_scrim_survives_a_resize() {
    let mut stage = staged!(rename_dialog);
    stage.wait(SETTLED);
    let at = aim(&stage, "Open dialog");
    stage.click(at);
    stage.wait(SETTLED);

    // A corner of the page, well away from the dialog: dimmed means the scrim is over it.
    let corner = zgui::geom::Point::new(
        zgui::geom::DevicePx(WIDTH - 30.0),
        zgui::geom::DevicePx(HEIGHT - 30.0),
    );
    let (red, green, blue) = stage.colour_at(corner);
    assert!(
        red < 240 && green < 240 && blue < 240,
        "the scrim is not dimming the page before the resize: ({red}, {green}, {blue})"
    );

    stage.resize(WIDTH + 80.0, HEIGHT + 40.0);
    stage.wait(Duration::from_millis(200));
    stage.capture("scrim-after-resize");
    let (red, green, blue) = stage.colour_at(corner);
    assert!(
        red < 240 && green < 240 && blue < 240,
        "the scrim vanished in the resize: ({red}, {green}, {blue})"
    );
}
