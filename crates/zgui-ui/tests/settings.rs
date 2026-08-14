//! A page of settings, driven: pages chosen, arrowed through, named and filled.
//!
//! Every test here mounts the real components through the ordinary view path, sends a real event,
//! and then asks the tree what changed. Nothing asserts that a view compiles, because a view that
//! compiles and does nothing compiles just as well.

mod harness;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::view;
use zgui::vocab::{NamedKey, SemanticFlags};
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// Every element under the root carrying `class`, in tree order.
fn all_with(harness: &Harness, class: &str) -> Vec<NodeId> {
    let name = zgui::view::ClassName::new(class);
    harness
        .all()
        .into_iter()
        .filter(|node| harness.window.dom.tree().classes(*node).contains(&name))
        .collect()
}

/// Only the focus moves a transcript recorded, which is what a keyboard test is about.
fn focus_moves(harness: &Harness) -> Vec<String> {
    harness
        .window
        .transcript
        .to_string()
        .lines()
        .filter(|line| line.starts_with("focus "))
        .map(str::to_owned)
        .collect()
}

/// Tells `node` that focus arrived on it, exactly as a window does after the keyboard moved it.
///
/// The move and the arrival are two things: a roving list asks the host to focus an entry, and the
/// window then delivers a focus event to whatever it focused. A test that only did the first would
/// never run a single `on:focus_in` handler.
fn focus_in(harness: &Harness, node: NodeId) {
    harness.window.dispatcher().send_to(
        node,
        zgui::vocab::EventKind::FocusIn,
        zgui::vocab::Payload::Focus(zgui::vocab::FocusEvent::new(
            zgui::vocab::FocusCause::Keyboard,
        )),
    );
    harness.window.frame();
}

/// What a relation names, as the identifiers a reader would follow.
fn ids(nodes: &[NodeId]) -> Vec<zgui::vocab::NodeId> {
    nodes
        .iter()
        .map(|node| zgui::vocab::NodeId(node.as_u64()))
        .collect()
}

/// Two pages of settings that own their own page choice, and the handles of their two entries.
fn settings(harness: &Harness) -> Vec<NodeId> {
    harness.mount(|| {
        view! {
            Settings(default_page = "appearance", label = "Preferences") {
                SettingsPages(label = "Pages") {
                    SettingsPage(value = "appearance") {"Appearance"}
                    SettingsPage(value = "terminal") {"Terminal"}
                }
                SettingsPane(value = "appearance") {text {"Colours"}}
                SettingsPane(value = "terminal") {text {"Shell"}}
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let entries = all_with(harness, "zui-settings__page");
    assert_eq!(entries.len(), 2);
    entries
}

// ---- the page choice ---------------------------------------------------------------------------

#[test]
fn the_page_it_opens_on_is_the_one_it_was_given_and_a_press_moves_it() {
    let harness = Harness::open();
    let entries = settings(&harness);
    let root = harness.only_child();

    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("appearance")
    );
    assert_eq!(
        harness.attribute(entries[0], "data-state").as_deref(),
        Some("active")
    );

    harness.click(entries[1]);

    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("terminal")
    );
    assert_eq!(
        harness.attribute(entries[1], "data-state").as_deref(),
        Some("active")
    );
    assert_eq!(
        harness.attribute(entries[0], "data-state").as_deref(),
        Some("inactive")
    );
}

#[test]
fn only_the_pane_being_looked_at_has_its_content_built() {
    let harness = Harness::open();
    let entries = settings(&harness);
    let panes = all_with(&harness, "zui-settings__pane");
    assert_eq!(panes.len(), 2);

    assert_eq!(harness.window.dom.tree().text_content(panes[0]), "Colours");
    assert_eq!(
        harness.window.dom.tree().text_content(panes[1]),
        "",
        "the page nobody is looking at is costing nothing"
    );

    harness.click(entries[1]);

    assert_eq!(harness.window.dom.tree().text_content(panes[1]), "Shell");
    assert_eq!(harness.window.dom.tree().text_content(panes[0]), "");
}

#[test]
fn a_pane_that_is_kept_stays_built_while_another_page_is_showing_and_still_says_it_is_hidden() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Settings(default_page = "appearance") {
                SettingsPages {
                    SettingsPage(value = "appearance") {"Appearance"}
                    SettingsPage(value = "terminal") {"Terminal"}
                }
                SettingsPane(value = "appearance") {text {"Colours"}}
                SettingsPane(value = "terminal", keep_mounted = true) {text {"Shell"}}
            }
        }
    });
    let panes = all_with(&harness, "zui-settings__pane");

    assert_eq!(harness.window.dom.tree().text_content(panes[1]), "Shell");
    assert!(
        harness
            .semantics(panes[1])
            .flags
            .contains(SemanticFlags::HIDDEN),
        "a kept pane is still a pane nobody chose, and it is read out in full unless it says it \
         is hidden"
    );
}

#[test]
fn a_caller_who_binds_a_writable_signal_gets_a_page_choice_that_moves_and_writes_it_back() {
    let harness = Harness::open();
    let page = RwSignal::new_local("appearance".to_owned());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&seen);
    harness.mount(move || {
        view! {
            Settings(
                page = page,
                on_page_change = UnsyncCallback::new(move |value: String| {
                    record.borrow_mut().push(value);
                })
            ) {
                SettingsPages {
                    SettingsPage(value = "appearance") {"Appearance"}
                    SettingsPage(value = "terminal") {"Terminal"}
                }
                SettingsPane(value = "appearance") {text {"Colours"}}
                SettingsPane(value = "terminal") {text {"Shell"}}
            }
        }
    });
    let entries = all_with(&harness, "zui-settings__page");
    let root = harness.only_child();

    harness.click(entries[1]);
    assert_eq!(
        page.get_untracked(),
        "terminal",
        "the press reached the caller's signal"
    );
    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("terminal"),
        "and the page the press chose is the one showing"
    );
    assert_eq!(
        *seen.borrow(),
        ["terminal".to_owned()],
        "and the observer was told as well"
    );

    // Pressing the entry that is already showing changes nothing and tells nobody: a callback that
    // fires when nothing moved is a loop with any caller that echoes it back.
    harness.click(entries[1]);
    assert_eq!(*seen.borrow(), ["terminal".to_owned()]);

    // And the caller can still drive it from outside.
    page.set("appearance".to_owned());
    harness.window.frame();
    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("appearance")
    );
}

#[test]
fn a_caller_who_controls_the_page_is_told_and_the_component_waits() {
    let harness = Harness::open();
    let page = RwSignal::new_local("appearance".to_owned());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&seen);
    harness.mount(move || {
        let record = Rc::clone(&record);
        view! {
            Settings(
                page = Binding::controlled(page, move |value: String| {
                    record.borrow_mut().push(value);
                })
            ) {
                SettingsPages {
                    SettingsPage(value = "appearance") {"Appearance"}
                    SettingsPage(value = "terminal") {"Terminal"}
                }
                SettingsPane(value = "appearance") {text {"Colours"}}
                SettingsPane(value = "terminal") {text {"Shell"}}
            }
        }
    });
    let entries = all_with(&harness, "zui-settings__page");
    let root = harness.only_child();

    harness.click(entries[1]);
    assert_eq!(
        *seen.borrow(),
        ["terminal".to_owned()],
        "the caller was told"
    );
    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("appearance"),
        "and the component did not move on its own"
    );

    page.set("terminal".to_owned());
    harness.window.frame();
    assert_eq!(
        harness.attribute(root, "data-page").as_deref(),
        Some("terminal")
    );
}

// ---- the keyboard -------------------------------------------------------------------------------

#[test]
fn arrowing_the_page_list_shows_the_page_it_lands_on() {
    let harness = Harness::open();
    let entries = settings(&harness);
    assert_eq!(harness.semantics(entries[0]).selected, Some(true));

    harness.window.transcript.clear();
    harness.press(entries[0], NamedKey::ArrowDown);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", entries[1].as_u64())]
    );
    focus_in(&harness, entries[1]);

    assert_eq!(harness.semantics(entries[1]).selected, Some(true));
    assert_eq!(harness.semantics(entries[0]).selected, Some(false));

    // And back up again, which is the same key the other way round rather than a second rule.
    harness.press(entries[1], NamedKey::ArrowUp);
    focus_in(&harness, entries[0]);
    assert_eq!(harness.semantics(entries[0]).selected, Some(true));
}

#[test]
fn the_page_list_leaves_the_horizontal_arrows_to_whatever_is_around_it() {
    let harness = Harness::open();
    let entries = settings(&harness);
    harness.window.transcript.clear();

    harness.press(entries[0], NamedKey::ArrowRight);
    assert!(
        focus_moves(&harness).is_empty(),
        "a list beside a pane with a text field in it must not swallow the keys that move a caret"
    );
}

#[test]
fn the_ends_of_the_list_are_one_key_away_and_a_page_that_cannot_be_chosen_is_not_one_of_them() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Settings(default_page = "appearance") {
                SettingsPages {
                    SettingsPage(value = "appearance") {"Appearance"}
                    SettingsPage(value = "streaming") {"Streaming"}
                    SettingsPage(value = "experiments", disabled = true) {"Experiments"}
                }
                SettingsPane(value = "appearance") {text {"Colours"}}
                SettingsPane(value = "streaming") {text {"Logs"}}
                SettingsPane(value = "experiments") {text {"Nothing yet"}}
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let entries = all_with(&harness, "zui-settings__page");
    assert_eq!(entries.len(), 3);

    harness.window.transcript.clear();
    harness.press(entries[0], NamedKey::End);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", entries[1].as_u64())],
        "the last page the keyboard may land on is the last one that can be chosen"
    );

    harness.window.transcript.clear();
    harness.press(entries[1], NamedKey::Home);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", entries[0].as_u64())]
    );

    // And a page that cannot be chosen does not move when it is pressed either.
    harness.click(entries[2]);
    assert_eq!(harness.semantics(entries[2]).selected, Some(false));
    assert!(
        harness
            .semantics(entries[2])
            .flags
            .contains(SemanticFlags::DISABLED)
    );
}

// ---- what a reader is told -----------------------------------------------------------------------

#[test]
fn the_page_list_is_a_vertical_list_of_tabs_and_each_entry_names_the_pane_it_shows() {
    let harness = Harness::open();
    let entries = settings(&harness);
    let panes = all_with(&harness, "zui-settings__pane");
    let list = harness.find("zui-settings__pages");

    assert_eq!(harness.semantics(list).role, Role::TabList);
    assert_eq!(harness.semantics(list).label.as_deref(), Some("Pages"));
    assert_eq!(
        harness.semantics(list).orientation,
        Some(zgui::vocab::Orientation::Vertical),
        "which arrow keys move within it is part of what the list is"
    );

    assert_eq!(harness.semantics(entries[0]).role, Role::Tab);
    assert_eq!(harness.semantics(panes[0]).role, Role::TabPanel);
    assert_eq!(
        harness.semantics(entries[0]).relations.controls,
        ids(&panes[..1])
    );
    assert_eq!(
        harness.semantics(panes[0]).relations.labelled_by,
        ids(&entries[..1]),
        "a pane is named by the entry that shows it rather than by a heading of its own"
    );
}

#[test]
fn a_group_is_named_and_described_by_the_writing_over_it() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SettingsGroup {
                SettingsGroupLabel {"Log streams"}
                SettingsGroupDescription {"What a stream does when the connection drops."}
                SettingsItem(label = "Reconnect attempts") {Switch()}
            }
        }
    });
    let group = harness.only_child();
    let label = harness.find("zui-settings__group-label");
    let description = harness.find("zui-settings__group-description");

    assert_eq!(harness.semantics(group).role, Role::Group);
    assert_eq!(
        harness.semantics(group).relations.labelled_by,
        ids(&[label])
    );
    assert_eq!(
        harness.semantics(group).relations.described_by,
        ids(&[description])
    );
}

#[test]
fn an_items_control_is_named_by_the_words_beside_it() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SettingsItem(
                label = "Traffic charts",
                description = "Sent and received traffic, in the port forward dialog."
            ) {
                Switch({..use_settings_item_attrs()})
            }
        }
    });
    let switch = harness.find("zui-switch");
    let label = harness.find("zui-label");
    let description = harness.find("zui-settings__item-description");

    assert_eq!(harness.semantics(label).role, Role::Label);
    assert_eq!(
        harness.window.dom.tree().text_content(label),
        "Traffic charts"
    );
    assert_eq!(
        harness.semantics(switch).role,
        Role::Switch,
        "the caller's control keeps everything it says about itself"
    );
    assert_eq!(
        harness.semantics(switch).relations.labelled_by,
        ids(&[label]),
        "a switch carries no words of its own, so without this it is announced as `switch` alone"
    );
    assert_eq!(
        harness.semantics(switch).relations.described_by,
        ids(&[description])
    );
}

#[test]
fn an_item_with_no_description_describes_its_control_as_nothing() {
    let harness = Harness::open();
    harness.mount(|| {
        view! { SettingsItem(label = "Dark mode") {Switch({..use_settings_item_attrs()})} }
    });
    let switch = harness.find("zui-switch");

    assert_eq!(
        harness.semantics(switch).relations.labelled_by.len(),
        1,
        "the name is there whether or not there is anything more to say"
    );
    assert!(
        harness.semantics(switch).relations.described_by.is_empty(),
        "a relation to a line that was never written is a question a reader gets no answer to"
    );
}

// ---- what an item holds --------------------------------------------------------------------------

#[test]
fn an_items_control_is_whatever_the_caller_wrote_there() {
    let harness = Harness::open();
    let text = RwSignal::new_local(String::new());
    harness.mount(move || {
        view! {
            SettingsGroup {
                SettingsItem(label = "Dark mode") {Switch({..use_settings_item_attrs()})}
                SettingsItem(label = "Custom shell") {
                    Input(value = text, {..use_settings_item_attrs()})
                }
                SettingsItem(label = "Reset") {Button {"Reset"}}
            }
        }
    });
    let slots = all_with(&harness, "zui-settings__item-control");
    assert_eq!(
        slots.len(),
        3,
        "one slot per item, in the order they were written"
    );

    let held = |slot: NodeId| harness.children(slot);
    assert_eq!(held(slots[0]).len(), 1);
    assert_eq!(harness.semantics(held(slots[0])[0]).role, Role::Switch);
    assert_eq!(harness.semantics(held(slots[1])[0]).role, Role::TextInput);
    assert_eq!(harness.semantics(held(slots[2])[0]).role, Role::Button);
    assert_eq!(
        harness.window.dom.tree().text_content(slots[2]),
        "Reset",
        "and the control keeps its own children"
    );
}

#[test]
fn an_item_that_is_out_of_action_says_so_and_stops_answering() {
    let harness = Harness::open();
    let off = RwSignal::new_local(true);
    harness.mount(move || {
        view! { SettingsItem(label = "Editor path", disabled = off) {Switch()} }
    });
    let item = harness.find("zui-settings__item");

    assert_eq!(
        harness.attribute(item, "data-disabled").as_deref(),
        Some("true")
    );

    off.set(false);
    harness.window.frame();
    assert_eq!(
        harness.attribute(item, "data-disabled").as_deref(),
        Some("false"),
        "the row follows the signal rather than the value it was built with"
    );
}

#[test]
fn a_press_on_the_words_moves_the_keyboard_to_the_control_they_name() {
    let harness = Harness::open();
    harness.mount(|| {
        let control = NodeRef::new();
        view! {
            SettingsItem(label = "Dark mode", control = control) {
                Switch(node_ref = control, {..use_settings_item_attrs()})
            }
        }
    });
    let label = harness.find("zui-label");
    let switch = harness.find("zui-switch");
    harness.window.transcript.clear();

    harness.click(label);

    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", switch.as_u64())],
        "a label is worth pressing only if the press reaches what it names"
    );
}
