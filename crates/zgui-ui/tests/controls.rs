//! The controls, driven: mounted, pressed, typed into, and read back.
//!
//! Every test here builds a real component through the ordinary view path, sends it a real event,
//! and then asks the tree what changed — the interaction states a style sheet selects on, the
//! semantics an accessibility tree is built from, the attributes a rule matches. Nothing asserts
//! that a view compiles, because a view that compiles and does nothing compiles just as well.

mod harness;

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::view;
use zgui::vocab::{NamedKey, SemanticFlags, Toggled, UiState};
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// A callback that writes down everything it was told.
fn recorder<T: 'static>() -> (UnsyncCallback<T>, std::rc::Rc<std::cell::RefCell<Vec<T>>>) {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let record = std::rc::Rc::clone(&seen);
    (
        UnsyncCallback::new(move |value: T| record.borrow_mut().push(value)),
        seen,
    )
}

// ---- button -----------------------------------------------------------------------------------

#[test]
fn a_button_is_pressed_by_the_space_bar_without_handling_a_single_key() {
    // The behaviour under test is the framework's, not the component's: activating whatever has
    // focus is what Enter and Space mean, and a button that handled them itself would be a second
    // answer that drifts. So this asserts the click arrived, having sent only a key.
    let harness = Harness::open();
    let presses = RwSignal::new_local(0);
    harness.mount(move || {
        view! { Button(on:click = move |_| presses.update(|n| *n += 1)) {"Save"} }
    });
    let button = harness.only_child();

    harness.press(button, NamedKey::Space);
    assert_eq!(presses.get_untracked(), 1);
    harness.press(button, NamedKey::Enter);
    assert_eq!(presses.get_untracked(), 2);
}

#[test]
fn a_disabled_button_asserts_the_state_a_sheet_and_a_reader_both_read() {
    let harness = Harness::open();
    let off = RwSignal::new_local(false);
    harness.mount(move || view! { Button(disabled = off) {"Save"} });
    let button = harness.only_child();

    assert!(!harness.state(button).contains(UiState::DISABLED));
    assert!(
        !harness
            .semantics(button)
            .flags
            .contains(SemanticFlags::DISABLED)
    );

    off.set(true);
    harness.window.frame();

    assert!(
        harness.state(button).contains(UiState::DISABLED),
        "`:disabled` is what fades the button, and there is no second signal that could say so"
    );
    assert!(
        harness
            .semantics(button)
            .flags
            .contains(SemanticFlags::DISABLED),
        "the appearance changed and the accessibility tree did not"
    );
}

#[test]
fn a_button_carries_its_variant_as_something_a_sheet_can_select_on() {
    let harness = Harness::open();
    harness.mount(|| {
        view! { Button(variant = ButtonVariant::Destructive, size = ButtonSize::Sm) {"Delete"} }
    });
    let button = harness.only_child();

    assert_eq!(
        harness.attribute(button, "data-variant").as_deref(),
        Some("destructive")
    );
    assert_eq!(
        harness.attribute(button, "data-size").as_deref(),
        Some("sm")
    );
    assert_eq!(harness.semantics(button).role, Role::Button);
}

#[test]
fn what_a_caller_forwards_beats_what_the_button_said() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Button(class = "mine", attr:data-testid = "save", a11y:label = "Save changes") {"Save"}
        }
    });
    let button = harness.only_child();

    assert!(
        harness
            .window
            .dom
            .tree()
            .classes(button)
            .contains(&zgui::view::ClassName::new("mine"))
    );
    assert_eq!(
        harness.attribute(button, "data-testid").as_deref(),
        Some("save")
    );
    assert_eq!(
        harness.semantics(button).label.as_deref(),
        Some("Save changes")
    );
    assert_eq!(
        harness.semantics(button).role,
        Role::Button,
        "a caller who named a label should not have turned the button into a box"
    );
}

// ---- checkbox ---------------------------------------------------------------------------------

#[test]
fn a_checkbox_toggles_from_the_keyboard_and_says_so_three_ways() {
    let harness = Harness::open();
    let (on_change, seen) = recorder::<Checked>();
    harness.mount(move || view! { Checkbox(on_change = on_change) });
    let box_ = harness.only_child();

    assert!(!harness.state(box_).contains(UiState::CHECKED));
    harness.press(box_, NamedKey::Space);

    assert!(harness.state(box_).contains(UiState::CHECKED));
    assert_eq!(harness.semantics(box_).toggled, Some(Toggled::True));
    assert_eq!(
        harness.attribute(box_, "data-state").as_deref(),
        Some("checked")
    );
    assert_eq!(*seen.borrow(), [Checked::Yes]);

    harness.press(box_, NamedKey::Space);
    assert!(!harness.state(box_).contains(UiState::CHECKED));
    assert_eq!(*seen.borrow(), [Checked::Yes, Checked::No]);
}

#[test]
fn a_mixed_checkbox_is_announced_as_mixed_and_becomes_ticked_when_pressed() {
    let harness = Harness::open();
    let state = RwSignal::new_local(Checked::Mixed);
    harness.mount(move || view! { Checkbox(checked = state) });
    let box_ = harness.only_child();

    assert_eq!(harness.semantics(box_).toggled, Some(Toggled::Mixed));
    assert!(
        harness.state(box_).contains(UiState::INDETERMINATE),
        "`:indeterminate` is what draws the dash instead of the tick"
    );

    harness.press(box_, NamedKey::Space);
    // The caller handed over a writable signal, so the press writes it: a part-way box becomes a
    // ticked one, and the signal the caller is reading elsewhere says so.
    assert_eq!(state.get_untracked(), Checked::Yes);
    assert!(harness.state(box_).contains(UiState::CHECKED));
    assert!(!harness.state(box_).contains(UiState::INDETERMINATE));
}

#[test]
fn a_checkbox_the_caller_controls_reports_the_press_and_shows_what_the_caller_says() {
    // The other half of the binding: a value the caller computes, with the write side spelled
    // out. Here the caller declines, and the box stays exactly where it was.
    let harness = Harness::open();
    let held = RwSignal::new_local(Checked::No);
    let (on_change, seen) = recorder::<Checked>();
    harness.mount(move || {
        view! { Checkbox(checked = Binding::controlled(held, |_: Checked| {}), on_change = on_change) }
    });
    let box_ = harness.only_child();

    harness.press(box_, NamedKey::Space);
    assert_eq!(*seen.borrow(), [Checked::Yes], "the caller was told");
    assert!(
        !harness.state(box_).contains(UiState::CHECKED),
        "and nothing moved, because the caller declined"
    );

    // The caller accepts it, and now the box shows it.
    held.set(Checked::Yes);
    harness.window.frame();
    assert!(harness.state(box_).contains(UiState::CHECKED));
}

#[test]
fn a_disabled_checkbox_refuses_the_space_bar() {
    let harness = Harness::open();
    let (on_change, seen) = recorder::<Checked>();
    harness.mount(move || view! { Checkbox(disabled = true, on_change = on_change) });
    let box_ = harness.only_child();

    harness.press(box_, NamedKey::Space);
    assert!(seen.borrow().is_empty(), "a disabled checkbox changed");
    assert!(!harness.state(box_).contains(UiState::CHECKED));
}

#[test]
fn a_label_names_its_control_in_the_accessibility_tree() {
    // The relation a screenshot cannot show, and the one that decides whether a screen reader can
    // say what the checkbox is for.
    let harness = Harness::open();
    harness.mount(|| {
        let name = NodeRef::new();
        let box_ = NodeRef::new();
        view! {
            row {
                Checkbox(node_ref = box_, labelled_by = name)
                Label(node_ref = name, control = box_) {"I accept the terms"}
            }
        }
    });
    let checkbox = harness.find("zui-checkbox");
    let label = harness.find("zui-label");

    assert_eq!(
        harness.semantics(checkbox).relations.labelled_by,
        vec![zgui::vocab::NodeId(label.as_u64())],
        "the checkbox does not name the label, so a reader cannot say what it is for"
    );
    assert_eq!(harness.semantics(label).role, Role::Label);
}

// ---- switch -----------------------------------------------------------------------------------

#[test]
fn a_switch_is_a_switch_to_a_reader_and_flips_on_enter() {
    let harness = Harness::open();
    let on = RwSignal::new_local(false);
    harness.mount(move || {
        view! { Switch(on_change = UnsyncCallback::new(move |next: bool| on.set(next))) }
    });
    let switch = harness.only_child();

    assert_eq!(harness.semantics(switch).role, Role::Switch);
    harness.press(switch, NamedKey::Enter);

    assert!(on.get_untracked());
    assert_eq!(harness.semantics(switch).toggled, Some(Toggled::True));
    assert!(harness.state(switch).contains(UiState::CHECKED));
}

// ---- radio group ------------------------------------------------------------------------------

#[test]
fn a_radio_group_chooses_as_the_arrow_keys_move_through_it() {
    let harness = Harness::open();
    let (on_change, seen) = recorder::<String>();
    harness.mount(move || {
        view! {
            RadioGroup(label = "Billing", on_change = on_change) {
                RadioGroupItem(value = "monthly", label = "Monthly")
                RadioGroupItem(value = "yearly", label = "Yearly")
            }
        }
    });
    let group = harness.only_child();
    let items: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-radio-group__item"))
        })
        .collect();
    assert_eq!(items.len(), 2, "two choices were written");

    assert_eq!(harness.semantics(group).role, Role::RadioGroup);
    assert_eq!(harness.semantics(items[0]).role, Role::RadioButton);

    // Focus reaches the first item, and the group is what the arrow keys are delivered to.
    harness.window.dispatcher().send_to(
        items[0],
        zgui::vocab::EventKind::FocusIn,
        zgui::vocab::Payload::Focus(zgui::vocab::FocusEvent::new(
            zgui::vocab::FocusCause::Keyboard,
        )),
    );
    harness.window.frame();
    assert_eq!(*seen.borrow(), ["monthly"]);
    assert!(harness.state(items[0]).contains(UiState::CHECKED));

    harness.press(items[1], NamedKey::ArrowDown);
    // Arrowing moved the roving tab stop; the item it landed on chooses when it is focused, which
    // is what this then drives.
    harness.window.dispatcher().send_to(
        items[1],
        zgui::vocab::EventKind::FocusIn,
        zgui::vocab::Payload::Focus(zgui::vocab::FocusEvent::new(
            zgui::vocab::FocusCause::Keyboard,
        )),
    );
    harness.window.frame();

    assert_eq!(*seen.borrow(), ["monthly", "yearly"]);
    assert!(harness.state(items[1]).contains(UiState::CHECKED));
    assert!(
        !harness.state(items[0]).contains(UiState::CHECKED),
        "two radio buttons in one group are checked at once"
    );
}

#[test]
fn a_radio_item_names_the_group_it_belongs_to() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            RadioGroup(label = "Size") {
                RadioGroupItem(value = "small", label = "Small")
            }
        }
    });
    let group = harness.only_child();
    let item = harness.find("zui-radio-group__item");

    assert_eq!(
        harness.semantics(item).relations.radio_group,
        vec![zgui::vocab::NodeId(group.as_u64())]
    );
}

// ---- toggle group -----------------------------------------------------------------------------

#[test]
fn a_single_selection_toggle_group_turns_the_last_one_off() {
    let harness = Harness::open();
    let (on_change, seen) = recorder::<Vec<String>>();
    harness.mount(move || {
        view! {
            ToggleGroup(label = "Alignment", on_change = on_change) {
                ToggleGroupItem(value = "left", label = "Left") {"L"}
                ToggleGroupItem(value = "right", label = "Right") {"R"}
            }
        }
    });
    let items: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-toggle-group__item"))
        })
        .collect();

    harness.press(items[0], NamedKey::Space);
    assert!(harness.state(items[0]).contains(UiState::CHECKED));

    harness.press(items[1], NamedKey::Space);
    assert!(
        !harness.state(items[0]).contains(UiState::CHECKED),
        "two alternatives are on at once, which is what single selection means it must not be"
    );
    assert!(harness.state(items[1]).contains(UiState::CHECKED));
    assert_eq!(
        *seen.borrow(),
        [vec!["left".to_owned()], vec!["right".to_owned()]]
    );
}

#[test]
fn a_multiple_selection_toggle_group_keeps_both_on() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            ToggleGroup(selection = ToggleSelection::Multiple, label = "Formatting") {
                ToggleGroupItem(value = "bold", label = "Bold") {"B"}
                ToggleGroupItem(value = "italic", label = "Italic") {"I"}
            }
        }
    });
    let items: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-toggle-group__item"))
        })
        .collect();

    harness.press(items[0], NamedKey::Space);
    harness.press(items[1], NamedKey::Space);

    assert!(harness.state(items[0]).contains(UiState::CHECKED));
    assert!(harness.state(items[1]).contains(UiState::CHECKED));
    assert_eq!(
        harness.semantics(items[0]).role,
        Role::Button,
        "a multiple-selection item is a pressed button rather than one of a set of alternatives"
    );
}

// ---- slider -----------------------------------------------------------------------------------

#[test]
fn a_slider_moves_by_its_step_and_stops_at_its_ends() {
    let harness = Harness::open();
    let value = RwSignal::new_local(0.0);
    harness.mount(move || {
        view! {
            Slider(
                min = 0.0,
                max = 10.0,
                step = 5.0,
                label = "Volume",
                on_change = UnsyncCallback::new(move |next: f64| value.set(next))
            )
        }
    });
    let slider = harness.only_child();

    assert_eq!(harness.semantics(slider).role, Role::Slider);
    assert_eq!(harness.semantics(slider).numeric.value, Some(0.0));

    harness.press(slider, NamedKey::ArrowRight);
    assert_eq!(value.get_untracked(), 5.0);
    assert_eq!(harness.semantics(slider).numeric.value, Some(5.0));

    harness.press(slider, NamedKey::End);
    assert_eq!(value.get_untracked(), 10.0);

    harness.press(slider, NamedKey::ArrowRight);
    assert_eq!(value.get_untracked(), 10.0, "it went past its own maximum");

    harness.press(slider, NamedKey::Home);
    assert_eq!(value.get_untracked(), 0.0);
    assert_eq!(
        (
            harness.semantics(slider).numeric.min,
            harness.semantics(slider).numeric.max,
            harness.semantics(slider).numeric.step
        ),
        (Some(0.0), Some(10.0), Some(5.0)),
        "a slider that does not report its range cannot be operated without looking at it"
    );
}

#[test]
fn the_fraction_a_slider_draws_itself_from_follows_its_value() {
    let harness = Harness::open();
    harness.mount(|| view! { Slider(default_value = 25.0, min = 0.0, max = 100.0, step = 25.0) });
    let slider = harness.only_child();
    let fraction = |harness: &Harness| {
        harness
            .window
            .dom
            .tree()
            .custom_property(
                slider,
                zgui::view::CustomPropertyName::new("zui-slider-fraction"),
            )
            .expect("the slider writes the fraction it draws itself from")
    };

    assert_eq!(fraction(&harness), "25.0000%");
    harness.press(slider, NamedKey::ArrowRight);
    assert_eq!(fraction(&harness), "50.0000%");
}

// ---- input ------------------------------------------------------------------------------------

/// Everything an element says, which for a field is the text the editing model wrote into it.
fn text_of(harness: &Harness, node: NodeId) -> String {
    harness.window.dom.tree().text_content(node)
}

#[test]
fn typing_into_a_field_puts_the_letters_in_the_field_and_tells_whoever_asked() {
    // The text is read out of the tree, never out of the callback: a field whose own element still
    // says nothing is a field showing its placeholder, and a component that reported perfectly
    // while displaying nothing is exactly the defect this asserts against.
    let harness = Harness::open();
    let (on_change, seen) = recorder::<String>();
    harness.mount(move || view! { Input(label = "Name", on_change = on_change) });
    let field = harness.only_child();

    assert_eq!(text_of(&harness, field), "", "a field starts empty");

    harness.type_char(field, 'h');
    harness.type_char(field, 'i');

    assert_eq!(*seen.borrow(), ["h", "hi"]);
    assert_eq!(text_of(&harness, field), "hi");

    harness.press(field, NamedKey::Backspace);
    assert_eq!(*seen.borrow(), ["h", "hi", "h"]);
    assert_eq!(text_of(&harness, field), "h");
}

#[test]
fn a_field_carries_its_placeholder_where_one_rule_can_draw_every_instance() {
    // Not an element: a field's element holds the text nodes the editing model writes and nothing
    // else, which is what makes `:empty` mean "there is no text here". A box holding the
    // placeholder would make every field non-empty for ever.
    let harness = Harness::open();
    harness.mount(|| view! { Input(placeholder = "you@example.com", label = "Email") });
    let field = harness.only_child();

    assert_eq!(
        harness.window.dom.tree().custom_property(
            field,
            zgui::view::CustomPropertyName::new("zui-field-placeholder")
        ),
        Some("\"you@example.com\"".to_owned())
    );
    assert_eq!(
        harness.semantics(field).placeholder.as_deref(),
        Some("you@example.com"),
        "and a reader is told it too"
    );
    assert!(
        harness
            .window
            .dom
            .tree()
            .children(field)
            .iter()
            .all(|child| harness.window.dom.tree().element_name(*child).is_none()),
        "an element child would keep the field from ever being empty"
    );
}

#[test]
fn a_field_leaves_tab_and_escape_for_whatever_is_around_it() {
    // A field that claimed tab is a field nobody can leave without a mouse, and the failure is
    // invisible in every test that only asserts what typing does.
    let harness = Harness::open();
    let (on_change, seen) = recorder::<String>();
    harness.mount(move || view! { Input(on_change = on_change) });
    let field = harness.only_child();

    for key in [NamedKey::Tab, NamedKey::Escape, NamedKey::Enter] {
        let delivered = harness
            .window
            .dispatcher()
            .key(field, zgui::vocab::Key::Named(key));
        assert_eq!(
            delivered.default,
            zgui::vocab::DefaultAction::Allowed,
            "the field claimed {key:?}, so the framework's own behaviour for it never runs"
        );
    }
    assert!(
        seen.borrow().is_empty(),
        "one of those keys changed the text"
    );
}

#[test]
fn a_read_only_field_moves_its_caret_and_refuses_every_change() {
    let harness = Harness::open();
    let (on_change, seen) = recorder::<String>();
    harness.mount(move || {
        view! { Input(default_value = "locked", read_only = true, on_change = on_change) }
    });
    let field = harness.only_child();

    harness.type_char(field, 'x');
    harness.press(field, NamedKey::Backspace);
    assert!(seen.borrow().is_empty(), "a read-only field was changed");
    assert!(harness.state(field).contains(UiState::READ_ONLY));
    assert!(
        harness
            .semantics(field)
            .flags
            .contains(SemanticFlags::READ_ONLY)
    );

    // The caret still moves, which is what makes it readable rather than merely unusable.
    harness.press(field, NamedKey::Home);
    assert_eq!(text_of(&harness, field), "locked");
}

#[test]
fn a_textarea_takes_a_line_break_where_an_input_leaves_enter_alone() {
    let harness = Harness::open();
    let text = RwSignal::new_local(String::new());
    harness.mount(move || {
        view! {
            Textarea(on_change = UnsyncCallback::new(move |next: String| text.set(next)))
        }
    });
    let area = harness.only_child();

    harness.type_char(area, 'a');
    harness.press(area, NamedKey::Enter);
    harness.type_char(area, 'b');

    assert_eq!(text.get_untracked(), "a\nb");
    assert_eq!(harness.semantics(area).role, Role::MultilineTextInput);
}

// ---- one-time code ----------------------------------------------------------------------------

#[test]
fn a_code_field_fills_one_box_at_a_time_and_refuses_a_seventh_character() {
    let harness = Harness::open();
    let (on_complete, done) = recorder::<String>();
    harness
        .mount(move || view! { InputOtp(length = 3, label = "Code", on_complete = on_complete) });
    let field = harness.only_child();

    for character in ['1', '2'] {
        harness.type_char(field, character);
    }
    assert!(done.borrow().is_empty(), "it completed before it was full");

    // The box the next character goes into is marked, and it is the third.
    let slots: Vec<NodeId> = harness
        .all()
        .into_iter()
        .filter(|node| {
            harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-otp__slot"))
        })
        .collect();
    assert_eq!(slots.len(), 3);
    assert_eq!(
        harness.attribute(slots[2], "data-active").as_deref(),
        Some("")
    );
    assert_eq!(harness.attribute(slots[0], "data-active"), None);

    harness.type_char(field, '3');
    assert_eq!(*done.borrow(), ["123"]);

    harness.type_char(field, '4');
    assert_eq!(
        harness.semantics(field).value.as_deref(),
        Some("123"),
        "a character past the end overwrote the code instead of being refused"
    );

    harness.press(field, NamedKey::Backspace);
    assert_eq!(harness.semantics(field).value.as_deref(), Some("12"));
}
