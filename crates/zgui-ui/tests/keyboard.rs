//! What a person with no pointer at all can do.
//!
//! Every component in this library can be worked with a mouse, and every fixture that drives one
//! with a mouse says nothing about the other half of the audience. A control that cannot be reached
//! by <kbd>Tab</kbd>, or that can be reached and not operated, is a control that does not work —
//! and the way it fails is silent, because the pointer path goes on passing.
//!
//! So nothing here sends a pointer event of any kind. The window is opened, focus is put where the
//! keyboard alone would put it, and the state is read back afterwards.

mod desktop;

use zgui::prelude::*;
use zgui::vocab::{Modifiers, NamedKey};
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

/// The sheet: room to lay things out in, and nothing that could hide anything.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// Walks Tab until `wanted` has focus, and says whether it ever did.
///
/// Bounded well past the number of controls in any of these fixtures, so a focus order that loops
/// without ever reaching the control is a failed search rather than a hung test.
fn tab_to(stage: &mut Stage, wanted: &str) -> bool {
    for _ in 0..40 {
        stage.key(NamedKey::Tab);
        if stage.focused_text().trim() == wanted {
            return true;
        }
    }
    false
}

// ---- Tabs -------------------------------------------------------------------------------------

/// Three tabs and the panel each of them shows.
#[component]
fn TabsPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Button {"before"}
            Tabs(default_value = "account") {
                TabsList {
                    TabsTrigger(value = "account") {"Account"}
                    TabsTrigger(value = "billing") {"Billing"}
                    TabsTrigger(value = "team") {"Team"}
                }
                TabsContent(value = "account") {"Your account details"}
                TabsContent(value = "billing") {"Card ending 4242"}
                TabsContent(value = "team") {"Nobody else yet"}
            }
        }
    }
}

#[test]
fn a_tab_list_is_reached_by_tab_and_walked_by_the_arrows() {
    let mut stage = Stage::open(SHEET, || view! { TabsPage() });
    assert!(
        stage.shows("Your account details"),
        "the first panel is showing before anything is pressed"
    );

    // A tab list is one tab stop, and the arrows move within it. Reaching the *selected* tab is
    // the whole of what makes the arrows usable at all.
    assert!(
        tab_to(&mut stage, "Account"),
        "Tab never reached the tab list; focus went to {:?}",
        stage.focused_text()
    );

    stage.key(NamedKey::ArrowRight);
    stage.settle();
    assert!(
        stage.shows("Card ending 4242"),
        "the right arrow did not move to the second tab; focus is on {:?}",
        stage.focused_text()
    );
    assert!(
        !stage.shows("Your account details"),
        "the first panel is still showing, so nothing was switched"
    );

    stage.key(NamedKey::ArrowRight);
    stage.key(NamedKey::ArrowRight);
    stage.settle();
    assert!(
        stage.shows("Your account details"),
        "the arrows did not wrap round the end of the list"
    );
}

// ---- Accordion --------------------------------------------------------------------------------

/// Three questions, one answer open at a time.
#[component]
fn AccordionPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Accordion {
                AccordionItem(value = "ship") {
                    AccordionTrigger {"When does it ship?"}
                    AccordionContent {"Within two working days."}
                }
                AccordionItem(value = "back") {
                    AccordionTrigger {"Can I send it back?"}
                    AccordionContent {"Thirty days, no questions."}
                }
            }
        }
    }
}

#[test]
fn an_accordion_opens_and_closes_from_the_keyboard_alone() {
    let mut stage = Stage::open(SHEET, || view! { AccordionPage() });
    assert!(!stage.shows("Within two working days."), "it starts closed");

    assert!(
        tab_to(&mut stage, "When does it ship?"),
        "Tab never reached the first trigger; focus went to {:?}",
        stage.focused_text()
    );

    // Enter and Space both activate, and a component that answered only one of them would work for
    // half the people who never touch a mouse.
    stage.key(NamedKey::Enter);
    stage.settle();
    assert!(
        stage.shows("Within two working days."),
        "Enter on the trigger opened nothing"
    );

    stage.key(NamedKey::Space);
    stage.settle();
    assert!(
        !stage.shows("Within two working days."),
        "Space on the trigger closed nothing"
    );

    // The headers are one tab stop between them and the arrows walk from one to the next, which is
    // the arrangement the component chose. Either arrangement is defensible; what is not is a
    // second header that neither Tab nor an arrow can reach.
    stage.key(NamedKey::ArrowDown);
    assert_eq!(
        stage.focused_text().trim(),
        "Can I send it back?",
        "neither Tab nor the down arrow reached the second header"
    );
    stage.key(NamedKey::ArrowUp);
    assert_eq!(
        stage.focused_text().trim(),
        "When does it ship?",
        "the up arrow did not come back"
    );

    // And the whole group is left the way it was entered.
    stage.key_with(NamedKey::Tab, Modifiers::SHIFT);
    assert_ne!(
        stage.focused_text().trim(),
        "When does it ship?",
        "Shift+Tab did not leave the accordion at all"
    );
}

// ---- Slider -----------------------------------------------------------------------------------

/// One slider, with the value written out beside it so a fixture can read it.
#[component]
fn SliderPage(
    /// Where the slider is, written back whenever it moves.
    value: RwSignal<f64, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    let told = zgui::reactive::UnsyncCallback::new(move |now: f64| value.set(now));
    view! {
        column(class = "page") {
            Button {"before"}
            Slider(
                default_value = 40.0,
                on_change = told,
                min = 0.0,
                max = 100.0,
                step = 5.0,
                label = "Volume"
            )
        }
    }
}

#[test]
fn a_slider_is_reached_and_moved_by_the_keyboard_alone() {
    let value = RwSignal::new_local(40.0_f64);
    let mut stage = Stage::open(SHEET, move || view! { SliderPage(value = value) });

    // The slider has no text, so it is found by walking to it and asking the document which node
    // has focus rather than by what it says.
    let slider = stage
        .census()
        .nodes
        .iter()
        .find(|node| node.text.is_empty() && node.area() > 0.0 && node.rect.is_some())
        .map(|node| node.id);
    assert!(slider.is_some(), "the slider is laid out");

    let mut reached = false;
    for _ in 0..10 {
        stage.key(NamedKey::Tab);
        let before = value.get_untracked();
        stage.key(NamedKey::ArrowRight);
        stage.settle();
        if value.get_untracked() != before {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "no amount of tabbing reached a control the arrows moved"
    );

    assert_eq!(
        value.get_untracked(),
        45.0,
        "the right arrow moves by one step"
    );

    stage.key(NamedKey::ArrowLeft);
    stage.key(NamedKey::ArrowLeft);
    stage.settle();
    assert_eq!(value.get_untracked(), 35.0, "the left arrow moves back");

    stage.key(NamedKey::Home);
    stage.settle();
    assert_eq!(value.get_untracked(), 0.0, "Home goes to the minimum");

    stage.key(NamedKey::End);
    stage.settle();
    assert_eq!(value.get_untracked(), 100.0, "End goes to the maximum");
}
