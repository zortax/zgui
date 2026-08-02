//! Whether a component that renders can actually be worked.
//!
//! A surface that opens is not a surface that can be used. Every fixture here opens a real window,
//! makes the gesture a person makes, and then asks the question a person would ask afterwards:
//! *where is the keyboard now*, *what does the screen say*, *what would a reader be told*. Nothing
//! asserts that a mechanism ran.

mod desktop;

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::vocab::NamedKey;
use zgui::{component, view};
use zgui_ui::form::Validator;
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

/// Room to lay things out in, and nothing that could hide anything.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

// ---- a dropdown menu ---------------------------------------------------------------------------

/// A menu behind a button, with items whose names differ from the trigger's.
#[component]
fn MenuPage() -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Account"}
                DropdownMenuContent {
                    MenuItem {"Settings"}
                    MenuItem {"Billing"}
                    MenuItem {"Sign out"}
                }
            }
        }
    }
}

#[test]
fn opening_a_dropdown_menu_puts_the_keyboard_on_its_first_item() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    stage.click_saying("Account");
    stage.settle();

    assert_eq!(
        stage.focused_text().trim(),
        "Settings",
        "the menu opened with the keyboard still on the trigger, so every key inside it is dead"
    );
}

#[test]
fn an_open_dropdown_menu_announces_the_item_the_keyboard_is_on() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    stage.click_saying("Account");
    stage.settle();

    let announced = stage
        .announced_focus()
        .expect("the window told a reader where the keyboard is");
    assert_eq!(
        (announced.role.as_str(), announced.name.as_str()),
        ("MenuItem", "Settings"),
        "a reader is told the keyboard is on {announced:?}, which is not the menu's first item"
    );
}

#[test]
fn the_down_arrow_walks_an_open_dropdown_menu() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    stage.click_saying("Account");
    stage.settle();

    stage.key(NamedKey::ArrowDown);
    assert_eq!(stage.focused_text().trim(), "Billing");
    stage.key(NamedKey::ArrowDown);
    assert_eq!(stage.focused_text().trim(), "Sign out");
    stage.key(NamedKey::ArrowUp);
    assert_eq!(stage.focused_text().trim(), "Billing");
}

#[test]
fn typing_a_letter_in_an_open_dropdown_menu_jumps_to_the_item() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    stage.click_saying("Account");
    stage.settle();

    stage.type_char('b');
    assert_eq!(
        stage.focused_text().trim(),
        "Billing",
        "typeahead never reached the menu"
    );
}

#[test]
fn escape_closes_a_dropdown_menu_and_gives_the_trigger_back() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    stage.click_saying("Account");
    stage.settle();
    assert!(stage.shows("Settings"), "the menu opened");

    stage.key(NamedKey::Escape);
    stage.settle();

    assert!(!stage.shows("Settings"), "Escape left the menu open");
    assert_eq!(
        stage.focused_text().trim(),
        "Account",
        "closing the menu dropped the keyboard instead of returning it to the trigger"
    );
}

// ---- a menubar ---------------------------------------------------------------------------------

/// Two menus on a bar, with different items in each.
#[component]
fn BarPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Menubar(label = "Main") {
                MenubarMenu(value = "file") {
                    MenubarTrigger {"File"}
                    MenubarContent {
                        MenubarItem {"New window"}
                        MenubarItem {"Open recent"}
                    }
                }
                MenubarMenu(value = "edit") {
                    MenubarTrigger {"Edit"}
                    MenubarContent {
                        MenubarItem {"Undo typing"}
                        MenubarItem {"Redo typing"}
                    }
                }
            }
        }
    }
}

#[test]
fn the_right_arrow_in_an_open_menu_moves_along_the_bar() {
    let mut stage = Stage::open(SHEET, || view! { BarPage() });
    stage.click_saying("File");
    stage.settle();
    assert!(stage.shows("New window"), "the File menu opened");

    stage.key(NamedKey::ArrowRight);
    stage.settle();

    assert!(
        !stage.shows("New window"),
        "the right arrow left the File menu open"
    );
    assert!(
        stage.shows("Undo typing"),
        "the right arrow inside an open menu did not open the next menu on the bar"
    );
    assert_eq!(
        stage.focused_text().trim(),
        "Undo typing",
        "the keyboard did not follow into the menu the bar opened"
    );
}

#[test]
fn the_left_arrow_in_an_open_menu_moves_back_along_the_bar() {
    let mut stage = Stage::open(SHEET, || view! { BarPage() });
    stage.click_saying("Edit");
    stage.settle();
    assert!(stage.shows("Undo typing"), "the Edit menu opened");

    stage.key(NamedKey::ArrowLeft);
    stage.settle();

    assert!(
        stage.shows("New window"),
        "the left arrow inside an open menu did not open the menu before it on the bar"
    );
}

#[test]
fn opening_a_menubar_menu_puts_the_keyboard_on_its_first_item() {
    let mut stage = Stage::open(SHEET, || view! { BarPage() });
    stage.click_saying("File");
    stage.settle();

    assert_eq!(stage.focused_text().trim(), "New window");
}

// ---- a form ------------------------------------------------------------------------------------

/// One field with a rule it fails, and a button that sends the form.
#[component]
fn SignUpPage() -> impl IntoView {
    let email = RwSignal::new_local("not-an-address".to_owned());
    let check = Validator::new(move || {
        let value = email.get();
        if value.contains('@') {
            None
        } else {
            Some("That does not look like an address.".to_owned())
        }
    });

    view! {
        column(class = "page") {
            Form {
                FormField(name = "email", validate = check) {
                    FormItem {
                        FormLabel {"Email"}
                        FormDescription {"We only write about this account."}
                        FormMessage()
                    }
                }
                FormSubmit {"Sign up"}
            }
        }
    }
}

#[test]
fn pressing_submit_on_an_invalid_form_puts_the_message_on_the_screen() {
    let mut stage = Stage::open(SHEET, || view! { SignUpPage() });
    assert!(
        !stage.shows("That does not look like an address."),
        "the form complained before anything was pressed"
    );

    stage.click_saying("Sign up");
    stage.settle();

    assert!(
        stage.shows("That does not look like an address."),
        "the submit button was pressed and the page says nothing about what is wrong"
    );
}

#[test]
fn a_form_message_is_announced_as_an_alert_once_there_is_something_to_say() {
    let mut stage = Stage::open(SHEET, || view! { SignUpPage() });
    let says = |stage: &Stage| {
        stage
            .announced()
            .into_iter()
            .any(|node| node.name == "That does not look like an address.")
    };
    assert!(
        !says(&stage),
        "a reader was told what is wrong before anything was pressed"
    );

    stage.click_saying("Sign up");
    stage.settle();

    assert!(
        says(&stage),
        "the message is drawn and no reader is told it: a form that complains only in colour \
         complains to nobody who cannot see it"
    );
}

#[test]
fn a_valid_form_sends_and_an_invalid_one_does_not() {
    let sent = RwSignal::new_local(0_u32);
    let text = RwSignal::new_local("nope".to_owned());
    let check =
        Validator::new(move || (!text.get().contains('@')).then(|| "Needs an @.".to_owned()));
    let mut stage = Stage::open(SHEET, move || {
        let check = check.clone();
        view! {
            column(class = "page") {
                Form(on_submit = UnsyncCallback::new(move |()| sent.update(|count| *count += 1))) {
                    FormField(name = "email", validate = check) {
                        FormItem {FormMessage()}
                    }
                    FormSubmit {"Send"}
                }
            }
        }
    });

    stage.click_saying("Send");
    stage.settle();
    assert_eq!(sent.get_untracked(), 0, "an invalid form was sent anyway");

    text.set("ada@example.com".to_owned());
    stage.settle();
    stage.click_saying("Send");
    stage.settle();
    assert_eq!(sent.get_untracked(), 1, "a valid form refused to send");
}

// ---- a calendar --------------------------------------------------------------------------------

/// A month with a day already chosen.
#[component]
fn CalendarPage() -> impl IntoView {
    let chosen = RwSignal::new_local(Date::new(2026, 7, 15));
    view! {
        column(class = "page") {
            Calendar(
                value = chosen,
                label = "Arrival",
                today = Date::new(2026, 7, 24).expect("a real date"),
                on_change = UnsyncCallback::new(move |date: Option<Date>| chosen.set(date))
            )
        }
    }
}

#[test]
fn the_arrow_keys_move_a_calendars_focus_from_day_to_day() {
    let mut stage = Stage::open(SHEET, || view! { CalendarPage() });
    stage.click_saying("15");
    stage.settle();
    assert_eq!(stage.focused_text().trim(), "15", "the 15th was pressed");

    stage.key(NamedKey::ArrowRight);
    stage.settle();
    assert_eq!(
        stage.focused_text().trim(),
        "16",
        "the right arrow left the keyboard on the day it started on"
    );

    stage.key(NamedKey::ArrowDown);
    stage.settle();
    assert_eq!(stage.focused_text().trim(), "23", "a week on");

    stage.key(NamedKey::ArrowUp);
    stage.settle();
    assert_eq!(stage.focused_text().trim(), "16", "a week back");
}

#[test]
fn a_calendar_announces_the_whole_date_the_keyboard_lands_on() {
    let mut stage = Stage::open(SHEET, || view! { CalendarPage() });
    stage.click_saying("15");
    stage.settle();
    stage.key(NamedKey::ArrowRight);
    stage.settle();

    let announced = stage
        .announced_focus()
        .expect("the window told a reader where the keyboard is");
    assert_eq!(
        announced.role, "GridCell",
        "a day is announced as {announced:?}"
    );
    assert!(
        announced.name.contains("16") && announced.name.contains("July"),
        "a reader arrowing through a grid of numbers is told only {:?}, and a number on its own \
         is not a date",
        announced.name
    );
}

#[test]
fn arrowing_off_the_end_of_a_month_walks_into_the_next_one() {
    let mut stage = Stage::open(SHEET, || view! { CalendarPage() });
    stage.click_saying("15");
    stage.settle();
    assert!(stage.shows("July 2026"), "the calendar opened on July");

    for _ in 0..17 {
        stage.key(NamedKey::ArrowRight);
    }
    stage.settle();

    assert!(
        stage.shows("August 2026"),
        "arrowing past the end of July never reached August"
    );
    assert_eq!(stage.focused_text().trim(), "1", "landed on 1 August");
}

// ---- a drawer ----------------------------------------------------------------------------------

/// A drawer behind a trigger.
#[component]
fn DrawerPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Drawer {
                DrawerTrigger {"Share"}
                DrawerContent {
                    DrawerHeader {
                        DrawerTitle {"Share this invoice"}
                    }
                    DrawerFooter {DrawerClose {"Done"}}
                }
            }
        }
    }
}

#[test]
fn a_drawer_reopens_while_the_last_one_is_still_leaving() {
    // The scrim outlives the close: a modal surface is kept mounted through its exit animation, so
    // for a few frames after a drawer is dismissed there is still a full-window box over the
    // trigger. A press in that window has to reach the trigger, or a drawer worked quickly is a
    // drawer that opens every other time.
    let mut stage = Stage::open(SHEET, || view! { DrawerPage() });
    stage.click_saying("Share");
    stage.settle();
    assert!(stage.shows("Share this invoice"), "the drawer opened");

    stage.click_saying("Done");
    // Deliberately not settled: this is the frame the exit animation is running in.
    stage.click_saying("Share");
    stage.settle();

    assert!(
        stage.shows("Share this invoice"),
        "a press made while the last drawer was still leaving opened nothing"
    );
}

#[test]
fn a_drawer_opens_and_closes_ten_times_running() {
    let mut stage = Stage::open(SHEET, || view! { DrawerPage() });
    let mut missed = Vec::new();
    for cycle in 0..10 {
        stage.click_saying("Share");
        stage.settle();
        if !stage.shows("Share this invoice") {
            missed.push(cycle);
            continue;
        }
        stage.key(NamedKey::Escape);
        stage.settle();
        if stage.shows("Share this invoice") {
            missed.push(cycle);
        }
    }
    assert!(
        missed.is_empty(),
        "the drawer failed to open or to close on cycles {missed:?}"
    );
}

// ---- a label ------------------------------------------------------------------------------------

/// A menu whose items are grouped under a heading.
#[component]
fn LabelledMenuPage() -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Account"}
                DropdownMenuContent {
                    MenuLabel {"Signed in as ada"}
                    MenuItem {"Settings"}
                }
            }
        }
    }
}

#[test]
fn a_menu_heading_has_the_box_it_is_drawn_in() {
    let mut stage = Stage::open(SHEET, || view! { LabelledMenuPage() });
    stage.click_saying("Account");
    stage.settle();

    let heading = stage
        .census()
        .nodes
        .into_iter()
        .filter(|node| node.text.trim() == "Signed in as ada")
        // The outermost node whose text is only the heading, which is the element the heading is:
        // the text inside it is not an element and has no box of its own to report.
        .min_by_key(|node| node.depth)
        .expect("the heading is in the document");
    let rect = heading.rect.expect("the heading generated a box");
    assert!(
        rect.size.width.0 > 0.0 && rect.size.height.0 > 0.0,
        "the heading is drawn and measures {:?}: anything asking where it is — a probe, a test, \
         a caller placing something beside it — is told it is nowhere",
        rect.size
    );
}
