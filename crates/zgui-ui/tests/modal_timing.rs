//! Closing a modal surface at every moment its animations can be in, and pressing something after.
//!
//! # Why the moment matters
//!
//! A dialog is not a state, it is a sequence: the press that opens it writes a signal, an effect
//! mounts the surface, the cascade starts an entrance, the entrance ends, and later an exit runs
//! and something has to notice that it finished. A dismissal can land in any of those, and the
//! ones that go wrong are not the tidy ones. A key delivered in the same batch as the press that
//! opened the surface arrives before the surface exists. A second Escape delivered while the first
//! surface is still fading arrives while two layers are registered and one of them is already
//! going. Neither is reachable by a fixture that opens a surface, waits for everything to settle,
//! and only then presses a key.
//!
//! So every fixture here places its close at a stated offset from the enter and the exit, and the
//! offsets are chosen to land inside them: nothing at all, one frame, half a transition, the frame
//! it ends on, and well past it.
//!
//! # Why a control is pressed after every one
//!
//! Everything a modal surface installs is invisible: a focus trap, an entry on the dismissable
//! stack, a hold on the window's scroll lock, a scrim over the whole window. A photograph of a
//! closed dialog cannot disagree with any of them, and a window carrying one it never released
//! answers no press and no key ever again. The assertion is therefore never only that the surface
//! went — it is that an ordinary control behind it still counts a click afterwards.

mod desktop;

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::reactive::prelude::{Get, GetUntracked, Set};
use zgui::vocab::NamedKey;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::{Act, Stage};

/// The page every fixture is laid out on.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// The same, with an exit that outlasts anything a person would wait for.
///
/// Two classes rather than one so that it outranks the library's own rule for the surface, which
/// is what this has to replace: what the fixtures below it are about is a dismissal whose
/// animation does not finish in any reasonable time.
const NEVER_ENDING: &str = ":root { background-color: #ffffff; color: #101010 }
                            .page { padding: 24px; gap: 16px; align-items: flex-start }
                            .zui-overlay-scope .zui-surface {
                                animation-duration: 30s; transition-duration: 30s }
                            .zui-overlay-layer .zui-overlay-scrim {
                                animation-duration: 30s; transition-duration: 30s }";

/// How long everything the tokens animate takes, with room to spare.
const SETTLED: Duration = Duration::from_millis(400);

/// Where a close is placed, relative to whatever animation is running when it arrives.
///
/// Zero is the same frame — and, where a fixture bursts it, the same batch. The rest walk across a
/// transition that the tokens make 120ms long: one frame in, a third of the way, the frame it ends
/// on, and past it.
const OFFSETS: [Duration; 5] = [
    Duration::ZERO,
    Duration::from_millis(16),
    Duration::from_millis(40),
    Duration::from_millis(120),
    Duration::from_millis(260),
];

/// The label of the control every fixture presses between rounds.
const TALLY: &str = "Tally";

/// What that control has counted, which is on the page as text.
fn clicked(round: usize) -> String {
    format!("Clicks {round}")
}

/// A page with something to press on it, and a dialog over it.
#[component]
fn Page() -> impl IntoView {
    let clicks = RwSignal::new_local(0u32);
    view! {
        ThemeProvider {
            column(class = "page") {
                Button(on:click = move |_| clicks.set(clicks.get_untracked() + 1)) {{TALLY}}
                text {{move || format!("Clicks {}", clicks.get())}}
                Dialog {
                    DialogTrigger {"Open dialog"}
                    DialogContent {
                        DialogTitle {"Rename project"}
                    }
                }
            }
        }
    }
}

/// The same page, with a second dialog written inside the first one's content.
#[component]
fn NestedPage() -> impl IntoView {
    let clicks = RwSignal::new_local(0u32);
    view! {
        ThemeProvider {
            column(class = "page") {
                Button(on:click = move |_| clicks.set(clicks.get_untracked() + 1)) {{TALLY}}
                text {{move || format!("Clicks {}", clicks.get())}}
                Dialog {
                    DialogTrigger {"Open dialog"}
                    DialogContent {
                        DialogTitle {"Rename project"}
                        Dialog {
                            DialogTrigger {"Open inner"}
                            DialogContent {
                                DialogTitle {"Confirm rename"}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Whether something saying `text` is on the screen, by the box it occupies.
///
/// The smallest laid-out node that says it, which is the innermost: a surface that has gone leaves
/// no node saying anything at all, and one that is there has its title in a box with an area. The
/// outermost node carrying a title is a wrapper that generates no box of its own, so a question
/// asked of that one is answered "no" for a surface which is plainly on the screen.
fn on_screen(stage: &Stage, text: &str) -> bool {
    stage.census().control(text).is_some()
}

/// Presses the page's own control and asserts that it counted, which only a live window can do.
fn assert_the_page_still_answers(stage: &mut Stage, round: usize, when: &str) {
    stage.click_saying(TALLY);
    stage.hold(SETTLED);
    assert!(
        on_screen(stage, &clicked(round)),
        "the window stopped answering a press after {when}"
    );
}

#[test]
fn escape_in_the_same_batch_as_the_click_that_opened_the_dialog_closes_it() {
    // The shortest gap there is: both events are handed over together, so the Escape is routed
    // into whatever the click had produced by then. If that is a document in which the dialog does
    // not exist yet, the key reaches nothing, is not delivered again, and the dialog stays up over
    // a window that now belongs to it.
    let mut stage = Stage::open(SHEET, || view! { Page() });
    stage.hold(SETTLED);

    stage.burst(&[Act::ClickSaying("Open dialog"), Act::Key(NamedKey::Escape)]);
    stage.hold(SETTLED);

    assert!(
        !on_screen(&stage, "Rename project"),
        "an Escape that arrived in the same batch as the press was lost, and the dialog is still up"
    );
    assert_the_page_still_answers(
        &mut stage,
        1,
        "an Escape delivered with the press that opened",
    );
}

#[test]
fn a_dialog_closed_at_every_point_of_its_entrance_leaves_the_page_working() {
    let mut stage = Stage::open(SHEET, || view! { Page() });
    stage.hold(SETTLED);

    for (round, offset) in OFFSETS.into_iter().enumerate() {
        stage.click_saying("Open dialog");
        stage.hold(offset);
        stage.key(NamedKey::Escape);
        stage.hold(SETTLED);

        assert!(
            !on_screen(&stage, "Rename project"),
            "Escape {offset:?} after the press that opened left the dialog up"
        );
        assert_the_page_still_answers(&mut stage, round + 1, &format!("a close {offset:?} in"));
    }
}

#[test]
fn a_second_escape_at_every_point_of_the_exit_leaves_the_page_working() {
    // The other half of the sequence: a close that lands *inside an exit that is already running*.
    // Somebody who pressed Escape and saw the dialog still there presses it again, and the second
    // one arrives while the surface is mounted, on its way out, and still registered as a layer.
    // Whatever that Escape is taken to mean, the window has to be whole afterwards — and the
    // dialog has to open again next time round, which is what makes each round a real cycle.
    let mut stage = Stage::open(SHEET, || view! { Page() });
    stage.hold(SETTLED);

    for (round, offset) in OFFSETS.into_iter().enumerate() {
        stage.click_saying("Open dialog");
        stage.hold(SETTLED);
        assert!(
            on_screen(&stage, "Rename project"),
            "the dialog would not open again on round {}",
            round + 1
        );

        stage.key(NamedKey::Escape);
        stage.hold(offset);
        stage.key(NamedKey::Escape);
        stage.hold(SETTLED);

        assert!(
            !on_screen(&stage, "Rename project"),
            "a second Escape {offset:?} into the exit left the dialog up"
        );
        assert_the_page_still_answers(
            &mut stage,
            round + 1,
            &format!("a second Escape {offset:?} into the exit"),
        );
    }
}

#[test]
fn escape_closes_the_inner_surface_first_however_soon_the_next_one_arrives() {
    // Escape belongs to the innermost surface that is open, and a surface that has been told to
    // close is not open — however long it goes on fading. Get that wrong and the second Escape is
    // eaten by the surface that is already leaving, so the dialog behind it is never closed by the
    // keyboard at all.
    let mut stage = Stage::open(SHEET, || view! { NestedPage() });
    stage.hold(SETTLED);

    for (round, offset) in OFFSETS.into_iter().enumerate() {
        stage.click_saying("Open dialog");
        stage.hold(SETTLED);
        stage.click_saying("Open inner");
        stage.hold(SETTLED);
        assert!(
            on_screen(&stage, "Confirm rename"),
            "both surfaces are open"
        );

        stage.key(NamedKey::Escape);
        stage.hold(offset);
        assert!(
            on_screen(&stage, "Rename project"),
            "the first Escape took the dialog behind the inner surface with it"
        );

        stage.key(NamedKey::Escape);
        stage.hold(SETTLED);
        assert!(
            !on_screen(&stage, "Confirm rename"),
            "the inner surface is still there after two presses of Escape"
        );
        assert!(
            !on_screen(&stage, "Rename project"),
            "the second Escape, {offset:?} after the first, was eaten by the surface on its way out"
        );
        assert_the_page_still_answers(
            &mut stage,
            round + 1,
            &format!("two Escapes {offset:?} apart"),
        );
    }
}

#[test]
fn two_escapes_in_one_batch_close_both_nested_surfaces() {
    // The same claim with no gap at all, which is the case a frame cannot separate: both keys are
    // handed over together, and the second must be resolved against the stack the first one left.
    let mut stage = Stage::open(SHEET, || view! { NestedPage() });
    stage.hold(SETTLED);

    stage.click_saying("Open dialog");
    stage.hold(SETTLED);
    stage.click_saying("Open inner");
    stage.hold(SETTLED);

    stage.burst(&[Act::Key(NamedKey::Escape), Act::Key(NamedKey::Escape)]);
    stage.hold(SETTLED);

    assert!(
        !on_screen(&stage, "Confirm rename"),
        "the inner surface stayed up"
    );
    assert!(
        !on_screen(&stage, "Rename project"),
        "the second Escape of the batch never reached the dialog behind the inner surface"
    );
    assert_the_page_still_answers(&mut stage, 1, "two Escapes in one batch");
}

#[test]
fn a_dialog_whose_exit_never_ends_still_gives_the_window_back() {
    // The failure the rest of this file cannot cause on purpose: an exit that does not finish in
    // any reasonable time. Nothing is broken here — the sheet simply asks for a transition thirty
    // seconds long, which is what a dropped animation end looks like from the presence's side.
    //
    // A modal surface that stays mounted keeps its scrim over the whole window and its focus trap
    // around a subtree nobody can see, so this is the difference between a dismissal that looks
    // slow and a session that is over.
    let mut stage = Stage::open(NEVER_ENDING, || view! { Page() });
    stage.hold(SETTLED);

    stage.click_saying("Open dialog");
    stage.hold(SETTLED);
    assert!(on_screen(&stage, "Rename project"), "the dialog opened");

    stage.key(NamedKey::Escape);
    stage.hold(Duration::from_millis(300));
    assert!(
        on_screen(&stage, "Rename project"),
        "the exit was cut short, so a surface with an animation is not being given its time"
    );

    stage.hold(Duration::from_millis(1_200));
    assert!(
        !on_screen(&stage, "Rename project"),
        "the dismissal was asked for and never happened, and the window is still covered"
    );
    assert_the_page_still_answers(&mut stage, 1, "an exit that never ended");
}
