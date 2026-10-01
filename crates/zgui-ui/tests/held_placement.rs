//! A value that the last keyframe of a finished animation holds stays in force when its element is
//! restyled.
//!
//! A dialog is centred by the last keyframe of its entrance, which `animation-fill-mode: both`
//! holds. A restyle of the dialog after the entrance must not drop that keyframe.

mod desktop;

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::{HEIGHT, Stage, WIDTH};

/// The page the fixtures are laid out on.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }";

/// Longer than every entrance in the library.
const ENTERED: Duration = Duration::from_millis(800);

/// How far, in device pixels, the middle of the dialog may be from the middle of the window.
const SLACK: f32 = 2.0;

/// The title of the dialog.
const TITLE: &str = "Resize me";

/// A probe that slides 200 pixels and holds its last keyframe.
const HELD: &str = ".probe { width: 100px; height: 40px; animation: slide 100ms linear forwards }
                    .probe[data-tint=\"on\"] { background-color: #00ff00 }
                    @keyframes slide {
                        from { transform: translate(0px, 0px) }
                        to { transform: translate(200px, 0px) }
                    }";

/// Makes sure that the middle of the dialog is at the middle of the window.
fn assert_centred(stage: &Stage, when: &str) {
    let census = stage.census();
    let panel = census.panel(TITLE).expect("the dialog is open");
    let middle = panel.centre().expect("the dialog has a box");
    assert!(
        (middle.x.0 - WIDTH / 2.0).abs() <= SLACK && (middle.y.0 - HEIGHT / 2.0).abs() <= SLACK,
        "{when}, the middle of the dialog is at ({}, {})",
        middle.x.0,
        middle.y.0,
    );
}

#[test]
fn a_dialog_stays_centred_when_a_tab_inside_it_is_switched() {
    let mut stage = Stage::open(SHEET, || {
        let open: RwSignal<bool, LocalStorage> = RwSignal::new_local(true);
        view! {
            ThemeProvider(scheme = ColorScheme::Light) {
                Dialog(open = open) {
                    DialogContent {
                        DialogHeader { DialogTitle {{TITLE}} }
                        Tabs(default_value = "tall", label = "Pages") {
                            TabsList {
                                TabsTrigger(value = "tall") {"Tall"}
                                TabsTrigger(value = "short") {"Short"}
                            }
                            TabsContent(value = "tall") {
                                box(style:height = "360px") {}
                            }
                            TabsContent(value = "short") { Button {"Short page"} }
                        }
                    }
                }
            }
        }
    });
    stage.hold(ENTERED);
    assert_centred(&stage, "after the entrance");

    stage.click_saying("Short");
    stage.hold(ENTERED);
    assert_centred(&stage, "after the switch to the short tab");
}

#[test]
fn a_dialog_stays_centred_when_the_scheme_changes() {
    let mut stage = Stage::open(SHEET, || {
        let open: RwSignal<bool, LocalStorage> = RwSignal::new_local(true);
        let scheme: RwSignal<ColorScheme, LocalStorage> = RwSignal::new_local(ColorScheme::Light);
        view! {
            ThemeProvider(scheme = scheme) {
                Dialog(open = open) {
                    DialogContent {
                        DialogHeader { DialogTitle {{TITLE}} }
                        Button(on:click = move |_| scheme.set(ColorScheme::Dark)) {"Go dark"}
                    }
                }
            }
        }
    });
    stage.hold(ENTERED);
    assert_centred(&stage, "after the entrance");

    stage.click_saying("Go dark");
    stage.hold(ENTERED);
    assert_centred(&stage, "after the switch to the dark scheme");
}

#[test]
fn a_held_keyframe_stays_in_force_through_restyles_of_its_element() {
    let mut stage = Stage::open(HELD, || {
        let tint: RwSignal<bool, LocalStorage> = RwSignal::new_local(false);
        view! {
            column {
                control(on:click = move |_| tint.set(!tint.get_untracked())) {"Tint"}
                box(
                    class = "probe",
                    attr:data-tint = move || Some(if tint.get() { "on" } else { "off" }.to_owned()),
                ) {"Probe"}
            }
        }
    });
    stage.hold(ENTERED);
    let at = |stage: &Stage| {
        let census = stage.census();
        let probe = census.control("Probe").expect("the probe is on the page");
        probe.rect.expect("the probe has a box").origin.x.0
    };
    let held = at(&stage);

    // Two restyles: the first drops the animation, and the second cascades without it.
    for _ in 0..2 {
        stage.click_saying("Tint");
        stage.hold(ENTERED);
    }
    assert_eq!(at(&stage), held, "the last keyframe is still in force");
}
