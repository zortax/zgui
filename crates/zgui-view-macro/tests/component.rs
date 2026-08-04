//! What `#[component]`, `#[slot]` and a `view!` over them actually do when they are run.

// The root a `view!` expansion names its crates through; an application gets it from
// the umbrella crate instead.
extern crate zgui_view as zgui;

mod support;

use std::cell::Cell;
use std::rc::Rc;

use support::Harness;
use zgui_reactive::prelude::*;
use zgui_reactive::{RwSignal, flush, on_cleanup_local};
use zgui_view::prelude::*;
use zgui_view_macro::{component, slot, view};

/// A component with one of every kind of prop.
#[component]
fn Label(
    /// What it says.
    #[prop(into)]
    text: String,
    /// Shown after the text when there is one.
    #[prop(into, optional)]
    hint: Option<String>,
    /// How many times it is repeated.
    #[prop(default = 1)]
    times: usize,
) -> impl IntoView {
    let repeated: String = text.repeat(times);
    view! { {repeated}{hint} }
}

#[test]
fn a_required_prop_is_given_and_the_others_default() {
    let harness = Harness::new();
    let _state = harness.mount(view! { Label(text = "a") });
    assert_eq!(harness.text(), "a");
}

#[test]
fn a_default_and_an_optional_prop_are_taken_when_they_are_written() {
    let harness = Harness::new();
    let _state = harness.mount(view! { Label(text = "a", times = 3, hint = "!") });
    assert_eq!(harness.text(), "aaa!");
}

#[test]
fn a_component_identifies_itself_by_module_path_and_name() {
    assert!(
        LabelProps::COMPONENT_ID.ends_with("::Label"),
        "{}",
        LabelProps::COMPONENT_ID
    );
}

/// A component says where it was written, not where the macro was.
///
/// What the inspector's component tree shows against each row, and the reason it is worth having:
/// "this row is a `Label`" is a smaller answer than "this row is the `Label` on line 21 of this
/// file", which is a place to put a cursor.
#[test]
fn a_component_says_which_file_and_line_it_was_declared_on() {
    let meta = LabelProps::COMPONENT_META;
    assert_eq!(meta.name, LabelProps::COMPONENT_ID);
    assert!(
        meta.file.ends_with("component.rs"),
        "a component declared in this file says it came from {}",
        meta.file
    );
    // The declaration is above this test, so its line is a real one and is before this one.
    assert!(meta.line > 0, "the line is {}", meta.line);
    assert!(
        meta.line < ::core::line!(),
        "the declaration is above this assertion but reports line {}",
        meta.line
    );
}

/// A component that registers a cleanup, to prove where its scope ends.
#[component]
fn Ephemeral(
    /// Incremented when this component's scope is cleaned up.
    cleaned: Rc<Cell<u32>>,
) -> impl IntoView {
    let counter = Rc::clone(&cleaned);
    on_cleanup_local(move || counter.set(counter.get() + 1));
    view! { "here" }
}

#[test]
fn a_components_scope_is_cleaned_up_synchronously_when_its_view_is_unmounted() {
    let harness = Harness::new();
    let cleaned = Rc::new(Cell::new(0));
    let mut state = harness.mount(view! { Ephemeral(cleaned = Rc::clone(&cleaned)) });
    assert_eq!(harness.text(), "here");
    assert_eq!(cleaned.get(), 0, "the scope is alive while the view is");

    state.unmount(&harness.dom);
    assert_eq!(cleaned.get(), 1, "and gone before `unmount` returns");
}

/// A component whose text follows a signal.
#[component]
fn Live(
    /// The text.
    text: RwSignal<String>,
) -> impl IntoView {
    view! { {move || text.get()} }
}

#[test]
fn a_prop_that_is_a_signal_drives_the_component_without_rebuilding_it() {
    let harness = Harness::new();
    let text = harness.window.with(|| RwSignal::new("before".to_owned()));
    let _state = harness.mount(view! { Live(text = text) });
    assert_eq!(harness.text(), "before");

    text.set("after".to_owned());
    flush();
    assert_eq!(harness.text(), "after");
}

/// The heading of a [`Card`].
#[slot]
struct CardHeader {
    /// What the heading shows.
    children: Children,
}

/// A card with an optional heading.
#[component(slot_aware)]
fn Card(
    /// The heading, when there is one.
    #[prop(optional)]
    card_header: Option<CardHeader>,
    children: Children,
) -> impl IntoView {
    let heading = card_header.map(|header| header.children.into_view_once());
    view! { {heading}{children.into_view_once()} }
}

#[test]
fn a_slot_child_fills_its_prop_and_the_rest_are_children() {
    let harness = Harness::new();
    let _state = harness.mount(view! {
        Card {
            CardHeader(slot) {"Total"}
            ": £12.00"
        }
    });
    assert_eq!(harness.text(), "Total: £12.00");
}

#[test]
fn a_card_without_a_heading_renders_only_its_children() {
    let harness = Harness::new();
    let _state = harness.mount(view! { Card {"bare"} });
    assert_eq!(harness.text(), "bare");
}

/// A component taking a closure argument for its children.
#[component]
fn Twice<F>(
    /// Called once per repetition, with the index.
    children: F,
) -> impl IntoView
where
    F: Fn(usize) -> AnyView + 'static,
{
    view! { {children(0)}{children(1)} }
}

#[test]
fn a_let_binding_names_the_argument_the_children_are_called_with() {
    let harness = Harness::new();
    let _state = harness.mount(view! {
        Twice(let:index) {{index.to_string()}}
    });
    assert_eq!(harness.text(), "01");
}

/// A component with a prop whose name is a keyword.
#[component]
fn Field(
    /// What kind of field it is.
    #[prop(into, name = "type")]
    kind: String,
) -> impl IntoView {
    view! { {kind} }
}

#[test]
fn a_renamed_prop_is_written_under_the_name_it_was_given() {
    let harness = Harness::new();
    let _state = harness.mount(view! { Field(type = "password") });
    assert_eq!(harness.text(), "password");
}

/// A component that listens for every event there is, to prove the table is not fiction.
#[component]
fn EveryEvent(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    view! { "" }
}

#[test]
fn every_event_name_the_grammar_accepts_resolves_to_a_constant() {
    let harness = Harness::new();
    let view = view! {
        EveryEvent(
            on:pointer_down = |_| {},
            on:pointer_up = |_| {},
            on:pointer_move = |_| {},
            on:pointer_enter = |_| {},
            on:pointer_leave = |_| {},
            on:pointer_cancel = |_| {},
            on:click = |_| {},
            on:double_click = |_| {},
            on:context_menu = |_| {},
            on:wheel = |_| {},
            on:key_down = |_| {},
            on:key_up = |_| {},
            on:text = |_| {},
            on:ime_start = |_| {},
            on:ime_preedit = |_| {},
            on:ime_commit = |_| {},
            on:ime_end = |_| {},
            on:focus_in = |_| {},
            on:focus_out = |_| {},
            on:drop = |_| {},
            on:input = |_| {},
            on:change = |_| {},
            on:scroll = |_| {},
            on:animation_start = |_| {},
            on:animation_iteration = |_| {},
            on:animation_end = |_| {},
            on:animation_cancel = |_| {},
            on:transition_run = |_| {},
            on:transition_start = |_| {},
            on:transition_end = |_| {},
            on:transition_cancel = |_| {}
        )
    };
    let _state = harness.mount(view);
}

/// A component whose children may be left out, with something to show when they are.
#[component]
fn Separator(
    /// What stands between the two things, when it is not the default.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    match children {
        Some(children) => children.into_view_once(),
        None => AnyView::new("/"),
    }
}

#[test]
fn children_that_may_be_left_out_are_still_written_as_children() {
    // The defect this catches is a prop declared `Option<Children>` whose setter takes a built
    // value rather than a closure: the component compiles, the caller does not, and the error
    // names a conversion nobody wrote.
    let harness = Harness::new();
    let _state = harness.mount(view! { Separator() });
    assert_eq!(harness.text(), "/");

    let harness = Harness::new();
    let _state = harness.mount(view! { Separator {"—"} });
    assert_eq!(harness.text(), "—");
}
