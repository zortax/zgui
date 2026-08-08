//! Whether a dialog keeps its words while it is being used.
//!
//! The report this reproduces: parts of an open dialog's text disappear, come back on a window
//! resize, and go again on the next hover. What was under it: the dialog's surface holds the
//! placement its enter animation settled on — a transform that lives only in the keyframes, kept
//! in force by `fill-mode: both` — so everything inside the surface is *drawn* through a matrix
//! while its rectangles were *measured* without one. Each reader of those rectangles that compared
//! them against device pixels lost the content its own way: the emit walk culled the lines away
//! (`Painter::where_drawn`), and the field's `overflow: hidden` clip was applied where the field
//! was laid out rather than where it is drawn, cutting its letters to nothing
//! (`ClipLink::RoundedRect::space`).
//!
//! A dialog is drawn once when it opens and then repainted piecewise as the pointer moves over its
//! controls — so the claim is made against the pixels after every one of those partial repaints,
//! not only against the frame that opened it: every word the dialog holds is still inked after
//! each hover, and after typing.

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::view;
use zgui::view::AnyView;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};
use crate::painted::words::{aim, ink};

/// The page the dialog opens over.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

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

/// Whether the words are inked where their box is, read off the accumulated pixels.
///
/// Deliberately not [`assert_painted`](crate::painted::words::assert_painted): that reads the
/// last *drawn* frame's display list, and once a caret is blinking in the dialog the last drawn
/// frame is routinely the blink — one quad, no glyphs, whatever the letters look like. The pixels
/// accumulate across frames and cannot be fooled by phase.
#[track_caller]
fn assert_inked(stage: &Stage, text: &str) {
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
        "{text:?} has a box at {rect:?} and no pixels in it: ink {coverage}"
    );
}

/// Every word the open dialog shows, asserted in one sweep.
fn assert_dialog_worded(stage: &Stage) {
    for words in [
        "Rename project",
        "Give it a name the invoices can carry.",
        "Cancel",
        "Rename",
    ] {
        assert_inked(stage, words);
    }
}

/// The whole report, on the real dialog: opened, hovered across, left, returned to, and typed in.
#[test]
fn a_dialog_keeps_its_words_under_the_pointer() {
    let mut stage = staged!(|| {
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Dialog {
                        DialogTrigger {"Open dialog"}
                        DialogContent {
                            DialogTitle {"Rename project"}
                            DialogDescription {"Give it a name the invoices can carry."}
                            Input(default_value = "Name", label = "Project name")
                            DialogFooter {
                                DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
                                Button {"Rename"}
                            }
                        }
                    }
                }
            }
        })
    });
    stage.wait(SETTLED);

    let at = aim(&stage, "Open dialog");
    stage.click(at);
    stage.wait(Duration::from_millis(400));
    stage.capture("dialog-opened");
    assert_dialog_worded(&stage);

    // The pointer wanders the way a person's does: over each control, resting long enough for
    // every hover transition to finish, with the words checked after every stop.
    for stop in ["Rename", "Cancel", "Name", "Rename project"] {
        let at = aim(&stage, stop);
        stage.move_to(at);
        stage.wait(Duration::from_millis(300));
        assert_dialog_worded(&stage);
    }

    // Leaving and returning is the other reported trigger.
    stage.leave();
    stage.wait(SETTLED);
    let at = aim(&stage, "Rename");
    stage.move_to(at);
    stage.wait(Duration::from_millis(300));
    assert_dialog_worded(&stage);

    // And the field takes what is typed, which is the rest of the report about this dialog.
    let at = aim(&stage, "Name");
    stage.click(at);
    stage.wait(SETTLED);
    stage.press_named(zgui::vocab::NamedKey::End);
    stage.type_text("d one");
    stage.wait(SETTLED);
    // The pixels themselves, deliberately without a repaint: the report is about the frame the
    // typing produced, and a full redraw would repair exactly the defect being looked for. The
    // ink threshold stands in for the glyph reading because the last *drawn* frame after a wait
    // can be a caret blink, whose display list holds no glyphs however well the letters painted.
    stage.capture("dialog-after-typing");
    assert_inked(&stage, "Named one");
    assert_dialog_worded(&stage);
}

/// The same typing, on a field standing on the page: whether a loss is the dialog's or typing's.
#[test]
fn a_page_field_paints_its_typing_without_a_repaint() {
    let mut stage = staged!(|| {
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Input(default_value = "Name", label = "Project name")
                }
            }
        })
    });
    stage.wait(SETTLED);

    let at = aim(&stage, "Name");
    stage.click(at);
    stage.wait(SETTLED);
    stage.press_named(zgui::vocab::NamedKey::End);
    stage.type_text("d one");
    stage.wait(SETTLED);
    assert_inked(&stage, "Named one");
}

/// A field inside a popover: the anchored surface with everything but the modal's machinery.
#[test]
fn a_popover_field_paints_its_words() {
    let mut stage = staged!(|| {
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Popover {
                        PopoverTrigger {"Open popover"}
                        PopoverContent {
                            Input(default_value = "Name", label = "Project name")
                        }
                    }
                }
            }
        })
    });
    stage.wait(SETTLED);
    let at = aim(&stage, "Open popover");
    stage.click(at);
    stage.wait(Duration::from_millis(400));
    stage.capture("popover-field");
    assert_inked(&stage, "Name");
}

/// The smallest surface that loses the field: a bare
/// [`ModalSurface`](zgui_ui::overlay::ModalSurface) wearing exactly what `DialogStyle` declares —
/// centred by `position: fixed` at 50 %, pulled back onto centre by the
/// `--zui-surface-place` transform the enter animation's fill mode holds, and padded.
///
/// Every piece short of this combination painted throughout the hunt: a portal, a popover, the
/// bare surface with and without its scrim, the centred surface, even the pulled-back surface
/// without padding. The padding is what moves the field's `overflow: hidden` clip away from the
/// surface's corner, so a clip applied without the transform no longer happens to cover the
/// letters — which is why this exact fixture is the one kept.
#[test]
fn a_dialog_look_alike_modal_surface_field_paints() {
    use zgui::vocab::Role;
    use zgui_ui::overlay::{ModalSurfaceProps, OverlayState};

    let mut stage = staged!(move || {
        let state = OverlayState::uncontrolled(false, None).provide();
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    control(node_ref = {state.trigger()}, on:click = move |_| state.open()) {
                        "Open surface"
                    }
                    ModalSurface(
                        state = state,
                        role = {Role::Dialog},
                        style:position = "fixed",
                        style:left = "50%",
                        style:top = "50%",
                        style:--zui-surface-place = "translate(-50%, -50%)",
                        style:padding = "8px"
                    ) {
                        Input(default_value = "Name", label = "Project name")
                    }
                }
            }
        })
    });
    stage.wait(SETTLED);
    let at = aim(&stage, "Open surface");
    stage.click(at);
    stage.wait(Duration::from_millis(500));
    stage.capture("modal-dialog-look");
    assert_inked(&stage, "Name");
}
