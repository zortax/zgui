//! Disclosure, driven: opened, closed, measured and arrowed through.
//!
//! Every test here mounts a real component through the ordinary view path, sends it a real event or
//! delivers a real observation, and then asks the tree what changed. Nothing asserts that a view
//! compiles, because a view that compiles and does nothing compiles just as well.

mod harness;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::vocab::{NamedKey, SemanticFlags};
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// What a node is publishing for one custom property.
fn custom(harness: &Harness, node: NodeId, property: &str) -> Option<String> {
    harness
        .window
        .dom
        .tree()
        .custom_property(node, zgui::view::CustomPropertyName::new(property))
}

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
///
/// A handler running is recorded too, and a handler that ran and decided to do nothing is exactly
/// what "this group leaves that key alone" looks like — so an assertion about the whole transcript
/// would fail for the right behaviour.
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
/// The move and the arrival are two things: a roving group asks the host to focus an item, and the
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

/// Delivers a border box to `node`, exactly as a completed layout does.
fn measure(harness: &Harness, node: NodeId, height: f32) {
    harness.window.dom.deliver(
        node,
        zgui::view::ObservedValue::BorderBox(zgui::geom::Rect::new(
            zgui::geom::Point::new(zgui::geom::DevicePx(0.0), zgui::geom::DevicePx(0.0)),
            zgui::geom::Size::new(zgui::geom::DevicePx(300.0), zgui::geom::DevicePx(height)),
        )),
    );
    harness.window.frame();
}

// ---- collapsible ------------------------------------------------------------------------------

#[test]
fn a_trigger_opens_the_content_and_says_so_to_a_reader() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Collapsible {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let trigger = harness.find("zui-collapsible__trigger");
    let content = harness.find("zui-collapsible__content");
    let root = harness.only_child();

    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("closed")
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert!(
        harness
            .semantics(content)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "content clipped to nothing is read out in full unless it says it is hidden"
    );

    harness.click(trigger);

    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("open")
    );
    assert_eq!(
        harness.attribute(content, "data-state").as_deref(),
        Some("open")
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(true));
    assert!(
        !harness
            .semantics(content)
            .flags
            .contains(SemanticFlags::HIDDEN)
    );
}

#[test]
fn the_trigger_names_the_element_it_controls_rather_than_repeating_its_text() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Collapsible {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let trigger = harness.find("zui-collapsible__trigger");
    let content = harness.find("zui-collapsible__content");

    assert_eq!(
        harness.semantics(trigger).relations.controls,
        [zgui::vocab::NodeId(content.as_u64())],
        "the relation is to the element, so renaming either cannot break it"
    );
}

#[test]
fn the_height_a_section_slides_to_is_the_one_it_measured() {
    // The defect this catches is a component with a pixel height in its style sheet, which is
    // wrong the first time the content changes — a longer line, a translation, a larger type scale.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Collapsible(default_open = true) {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let content = harness.find("zui-collapsible__content");
    let inner = harness.find("zui-collapsible__measure");

    assert_eq!(
        custom(&harness, content, "zui-collapsible-height"),
        None,
        "nothing is published before anything has been measured"
    );

    measure(&harness, inner, 84.0);
    assert_eq!(
        custom(&harness, content, "zui-collapsible-height").as_deref(),
        Some("84px")
    );

    // The content grew, and the number followed without anybody being told to re-measure.
    measure(&harness, inner, 132.0);
    assert_eq!(
        custom(&harness, content, "zui-collapsible-height").as_deref(),
        Some("132px")
    );
}

#[test]
fn the_measured_height_is_in_the_unit_the_style_sheet_is_written_in() {
    // A style sheet is written in CSS pixels and a box is measured in device pixels, so a component
    // that published the raw measurement would slide to twice the right height on a 2× display.
    let harness = Harness::open();
    harness.window.host.set_scale(2.0);
    harness.mount(|| {
        view! {
            Collapsible(default_open = true) {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let content = harness.find("zui-collapsible__content");
    let inner = harness.find("zui-collapsible__measure");

    measure(&harness, inner, 200.0);
    assert_eq!(
        custom(&harness, content, "zui-collapsible-height").as_deref(),
        Some("100px"),
        "200 device pixels at 2× is 100 CSS pixels"
    );
}

#[test]
fn a_disabled_disclosure_does_not_open_when_it_is_pressed() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Collapsible(disabled = true) {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let trigger = harness.find("zui-collapsible__trigger");
    let root = harness.only_child();

    harness.click(trigger);
    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("closed")
    );
    assert!(
        harness
            .semantics(trigger)
            .flags
            .contains(SemanticFlags::DISABLED)
    );
}

#[test]
fn a_caller_who_binds_a_writable_signal_gets_a_disclosure_that_opens_and_writes_it_back() {
    let harness = Harness::open();
    let open = RwSignal::new_local(false);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let record = std::rc::Rc::clone(&seen);
    harness.mount(move || {
        view! {
            Collapsible(
                open = open,
                on_open_change = zgui::reactive::UnsyncCallback::new(move |value: bool| {
                    record.borrow_mut().push(value);
                })
            ) {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let trigger = harness.find("zui-collapsible__trigger");
    let root = harness.only_child();

    harness.click(trigger);
    assert!(
        open.get_untracked(),
        "the press reached the caller's signal"
    );
    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("open"),
        "and the section the press opened is open"
    );
    assert_eq!(*seen.borrow(), [true], "and the observer was told as well");

    // And the caller can still drive it from outside.
    open.set(false);
    harness.window.frame();
    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("closed")
    );
}

#[test]
fn a_caller_who_controls_the_state_is_told_and_the_component_waits() {
    let harness = Harness::open();
    let open = RwSignal::new_local(false);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let record = std::rc::Rc::clone(&seen);
    harness.mount(move || {
        let record = std::rc::Rc::clone(&record);
        view! {
            Collapsible(
                open = Binding::controlled(open, move |value: bool| {
                    record.borrow_mut().push(value);
                })
            ) {
                CollapsibleTrigger {"Details"}
                CollapsibleContent {text {"Arrives Thursday."}}
            }
        }
    });
    let trigger = harness.find("zui-collapsible__trigger");
    let root = harness.only_child();

    harness.click(trigger);
    assert_eq!(*seen.borrow(), [true], "the caller was told");
    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("closed"),
        "and the component did not move on its own"
    );

    open.set(true);
    harness.window.frame();
    assert_eq!(
        harness.attribute(root, "data-state").as_deref(),
        Some("open")
    );
}

// ---- accordion --------------------------------------------------------------------------------

/// An accordion with three sections, and the handles of its three headings.
fn accordion(harness: &Harness, selection: AccordionSelection) -> Vec<NodeId> {
    harness.mount(move || {
        view! {
            Accordion(selection = selection) {
                AccordionItem(value = "one") {
                    AccordionTrigger {"One"}
                    AccordionContent {text {"First"}}
                }
                AccordionItem(value = "two") {
                    AccordionTrigger {"Two"}
                    AccordionContent {text {"Second"}}
                }
                AccordionItem(value = "three") {
                    AccordionTrigger {"Three"}
                    AccordionContent {text {"Third"}}
                }
            }
        }
    });
    // Tree order is what the arrow keys walk, and it is the engine's answer rather than the order
    // the items happened to register in.
    harness.window.host.set_tree_order(harness.all());
    let triggers = all_with(harness, "zui-accordion__trigger");
    assert_eq!(triggers.len(), 3, "three headings were written");
    triggers
}

#[test]
fn opening_one_section_of_a_single_selection_accordion_closes_the_other() {
    let harness = Harness::open();
    let triggers = accordion(&harness, AccordionSelection::Single);

    harness.click(triggers[0]);
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(true));
    assert_eq!(harness.semantics(triggers[1]).expanded, Some(false));

    harness.click(triggers[1]);
    assert_eq!(
        harness.semantics(triggers[0]).expanded,
        Some(false),
        "a section that owned its own answer could only have found out afterwards"
    );
    assert_eq!(harness.semantics(triggers[1]).expanded, Some(true));
}

#[test]
fn a_multiple_selection_accordion_keeps_both_open() {
    let harness = Harness::open();
    let triggers = accordion(&harness, AccordionSelection::Multiple);

    harness.click(triggers[0]);
    harness.click(triggers[2]);
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(true));
    assert_eq!(harness.semantics(triggers[2]).expanded, Some(true));
}

#[test]
fn the_arrow_keys_walk_the_headings_and_open_nothing_on_the_way() {
    let harness = Harness::open();
    let triggers = accordion(&harness, AccordionSelection::Single);
    harness.window.transcript.clear();

    harness.press(triggers[0], NamedKey::ArrowDown);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[1].as_u64())],
        "the arrow moved the focus"
    );
    assert!(
        (0..3).all(|index| harness.semantics(triggers[index]).expanded == Some(false)),
        "arrowing opened something; a reader cannot survey headings that open as they are passed"
    );

    harness.window.transcript.clear();
    harness.press(triggers[1], NamedKey::End);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[2].as_u64())]
    );

    harness.window.transcript.clear();
    harness.press(triggers[2], NamedKey::ArrowDown);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[0].as_u64())],
        "the group wraps"
    );
}

#[test]
fn an_accordion_heading_is_a_heading_with_a_button_in_it() {
    // Two elements on purpose: a reader jumping by heading meets every section's title, and the
    // button inside each one is what opens it. One element could be a heading or a button.
    let harness = Harness::open();
    let triggers = accordion(&harness, AccordionSelection::Single);
    let heading = harness.find("zui-accordion__header");

    assert_eq!(harness.semantics(heading).role, Role::Heading);
    assert_eq!(harness.semantics(triggers[0]).role, Role::Button);
    assert!(
        harness
            .window
            .dom
            .tree()
            .children(heading)
            .contains(&triggers[0])
    );
}

#[test]
fn an_accordion_section_slides_on_the_disclosure_it_is_built_out_of() {
    // Stated because it is the whole reason the accordion has no measuring code of its own: the
    // body *is* the collapsible's body, and it publishes the same property.
    let harness = Harness::open();
    let triggers = accordion(&harness, AccordionSelection::Single);
    harness.click(triggers[0]);

    let content = harness.find("zui-accordion__content");
    let inner = harness.find("zui-collapsible__measure");
    measure(&harness, inner, 60.0);

    assert_eq!(
        custom(&harness, content, "zui-collapsible-height").as_deref(),
        Some("60px")
    );
    assert_eq!(
        harness.semantics(content).role,
        Role::Region,
        "an opened section is a region a reader can jump to"
    );
}

// ---- tabs -------------------------------------------------------------------------------------

/// A tab set with two tabs, and the handles of its two tabs.
fn tabs(harness: &Harness, activation: TabsActivation) -> Vec<NodeId> {
    harness.mount(move || {
        view! {
            Tabs(default_value = "profile", activation = activation, label = "Account") {
                TabsList {
                    TabsTrigger(value = "profile") {"Profile"}
                    TabsTrigger(value = "billing") {"Billing"}
                }
                TabsContent(value = "profile") {text {"Your name"}}
                TabsContent(value = "billing") {text {"Your cards"}}
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let found = all_with(harness, "zui-tabs__trigger");
    assert_eq!(found.len(), 2);
    found
}

#[test]
fn a_tab_and_its_panel_name_each_other() {
    let harness = Harness::open();
    let triggers = tabs(&harness, TabsActivation::Automatic);
    let panels = all_with(&harness, "zui-tabs__content");
    assert_eq!(panels.len(), 2);

    assert_eq!(
        harness.semantics(triggers[0]).relations.controls,
        [zgui::vocab::NodeId(panels[0].as_u64())]
    );
    assert_eq!(
        harness.semantics(panels[0]).relations.labelled_by,
        [zgui::vocab::NodeId(triggers[0].as_u64())]
    );
    assert_eq!(harness.semantics(triggers[0]).role, Role::Tab);
    assert_eq!(harness.semantics(panels[0]).role, Role::TabPanel);
}

#[test]
fn only_the_selected_panel_has_its_content_built() {
    let harness = Harness::open();
    let triggers = tabs(&harness, TabsActivation::Automatic);
    let panels = all_with(&harness, "zui-tabs__content");

    assert_eq!(
        harness.window.dom.tree().text_content(panels[0]),
        "Your name"
    );
    assert_eq!(
        harness.window.dom.tree().text_content(panels[1]),
        "",
        "the panel nobody is looking at is costing nothing"
    );

    harness.click(triggers[1]);
    assert_eq!(
        harness.window.dom.tree().text_content(panels[1]),
        "Your cards"
    );
    assert_eq!(harness.window.dom.tree().text_content(panels[0]), "");
}

#[test]
fn arrowing_a_tab_strip_shows_the_panel_it_lands_on() {
    let harness = Harness::open();
    let triggers = tabs(&harness, TabsActivation::Automatic);
    assert_eq!(harness.semantics(triggers[0]).selected, Some(true));

    harness.press(triggers[0], NamedKey::ArrowRight);
    focus_in(&harness, triggers[1]);

    assert_eq!(harness.semantics(triggers[1]).selected, Some(true));
    assert_eq!(harness.semantics(triggers[0]).selected, Some(false));
}

#[test]
fn a_manual_strip_moves_without_showing_anything() {
    let harness = Harness::open();
    let triggers = tabs(&harness, TabsActivation::Manual);

    harness.press(triggers[0], NamedKey::ArrowRight);
    focus_in(&harness, triggers[1]);

    assert_eq!(
        harness.semantics(triggers[0]).selected,
        Some(true),
        "manual activation moved the selection anyway"
    );

    harness.click(triggers[1]);
    assert_eq!(harness.semantics(triggers[1]).selected, Some(true));
}

#[test]
fn a_vertical_strip_leaves_the_horizontal_arrows_to_whatever_is_around_it() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tabs(default_value = "profile", orientation = zgui_ui_primitives::Orientation::Vertical) {
                TabsList {
                    TabsTrigger(value = "profile") {"Profile"}
                    TabsTrigger(value = "billing") {"Billing"}
                }
                TabsContent(value = "profile") {text {"Your name"}}
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let triggers = all_with(&harness, "zui-tabs__trigger");
    harness.window.transcript.clear();

    harness.press(triggers[0], NamedKey::ArrowRight);
    assert!(
        focus_moves(&harness).is_empty(),
        "a vertical strip beside a scrolling panel must not swallow the keys that scroll it"
    );

    harness.press(triggers[0], NamedKey::ArrowDown);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[1].as_u64())]
    );
}

#[test]
fn the_ends_of_a_strip_are_one_key_away_and_a_disabled_tab_is_not_one_of_them() {
    // Home and End are half the roving-focus keyboard model, and a disabled tab is exactly the
    // thing they must not land on: a strip that shows whatever it lands on would leave the focus
    // ring on one tab and the panel of another, and Enter there would do nothing at all.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tabs(default_value = "profile", label = "Account") {
                TabsList {
                    TabsTrigger(value = "profile") {"Profile"}
                    TabsTrigger(value = "billing", disabled = true) {"Billing"}
                    TabsTrigger(value = "usage") {"Usage"}
                }
                TabsContent(value = "profile") {text {"Your name"}}
                TabsContent(value = "billing") {text {"Your cards"}}
                TabsContent(value = "usage") {text {"This month"}}
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let triggers = all_with(&harness, "zui-tabs__trigger");
    assert_eq!(triggers.len(), 3);

    harness.window.transcript.clear();
    harness.press(triggers[0], NamedKey::ArrowRight);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[2].as_u64())],
        "the arrow landed on the tab that cannot be chosen"
    );
    focus_in(&harness, triggers[2]);
    assert_eq!(
        harness.attribute(triggers[2], "data-state").as_deref(),
        Some("active"),
        "the strip shows whatever the arrows land on, so the panel moved with the focus"
    );

    harness.window.transcript.clear();
    harness.press(triggers[2], NamedKey::Home);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[0].as_u64())]
    );

    harness.window.transcript.clear();
    harness.press(triggers[0], NamedKey::End);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[2].as_u64())],
        "End reached past the last tab a user can actually use"
    );

    assert!(
        harness
            .semantics(triggers[1])
            .flags
            .contains(SemanticFlags::DISABLED),
        "skipped by the arrows, still announced: disabled is not absent"
    );
}
