//! The `{..attrs}` merge rules, as a transcript.
//!
//! Each rule is one case, and the transcript is what a component's root element would be told, in
//! the order it would be told it. The order is the contract: a component merges what its caller
//! forwarded *after* its own, so the caller is last and therefore wins.

// The root a `view!` expansion names its crates through; an application gets it from
// the umbrella crate instead.
extern crate zgui_view as zgui;

mod support;

use std::cell::{Cell, RefCell};

use zgui_view::prelude::*;
use zgui_view::{
    AttrEntry, AttrName, Attrs, ClassName, ListenerOptions, PropKey, PropValue, Role, UiState,
};
use zgui_view_macro::{component, view};

thread_local! {
    /// What the last built `Root` was handed.
    static TRANSCRIPT: RefCell<String> = const { RefCell::new(String::new()) };
    /// Every interaction state the last built `Root` was asked to assert, at once.
    static STATES: Cell<UiState> = const { Cell::new(UiState::EMPTY) };
}

/// A component with attributes of its own, which the caller's are merged after.
#[component]
fn Root(
    /// Merged after the component's own classes.
    #[prop(into, optional)]
    class: Classes,
    /// What the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let own = Attrs::new()
        .classes_from(Classes::from("root"))
        .class_toggle(ClassName::new("busy"), false)
        .style_property("gap", Some("1rem".to_owned()))
        .attribute(AttrName::new("data-part"), Some("root".to_owned()))
        .property(PropKey::new("value"), PropValue::Text("own".into()))
        .listener(events::CLICK, ListenerOptions::DEFAULT, |_| {})
        .a11y_from(A11yBinding::with_role(Role::Button).label("component"));
    let merged = own.merged(attrs).classes_from(class);
    STATES.with(|states| {
        states.set(
            merged
                .entries()
                .iter()
                .fold(UiState::EMPTY, |all, entry| match entry {
                    AttrEntry::StateToggle(state, _) => all | *state,
                    _ => all,
                }),
        );
    });
    TRANSCRIPT.with(|transcript| *transcript.borrow_mut() = describe(&merged));
    view! { "" }
}

/// Renders a bundle as the writes it stands for.
fn describe(attrs: &Attrs) -> String {
    let mut out = String::new();
    out.push_str("class list:");
    for name in attrs.classes().names() {
        out.push(' ');
        out.push_str(name.as_str());
    }
    out.push('\n');
    for entry in attrs.entries() {
        let line = match entry {
            AttrEntry::ClassToggle(name, on) => {
                format!("class-toggle {} = {}", name.as_str(), on.get())
            }
            AttrEntry::StyleProperty(property, value) => {
                format!("style {property} = {:?}", value.get())
            }
            AttrEntry::CustomProperty(property, value) => {
                format!("custom-property {} = {:?}", property.as_str(), value.get())
            }
            AttrEntry::Attribute(name, value) => {
                format!("attribute {} = {:?}", name.as_str(), value.get())
            }
            AttrEntry::StateToggle(state, on) => format!("state {state:?} = {}", on.get()),
            AttrEntry::CustomStateToggle(name, on) => {
                format!("custom-state {} = {}", name.as_str(), on.get())
            }
            AttrEntry::Property(key, value) => {
                format!("property {} = {:?}", key.as_str(), value.get())
            }
            AttrEntry::Listener(kind, options, _) => {
                format!("listener {} capture={}", kind.name(), options.capture)
            }
        };
        out.push_str(&line);
        out.push('\n');
    }
    if let Some(a11y) = attrs.a11y() {
        let semantics = a11y.lower();
        out.push_str(&format!(
            "a11y role={:?} label={:?}\n",
            semantics.role, semantics.label
        ));
    }
    out
}

/// Builds one case and hands back what the component was told.
fn transcript_of(view: impl IntoView) -> String {
    let harness = support::Harness::new();
    let _state = harness.mount(view);
    TRANSCRIPT.with(|transcript| transcript.borrow().clone())
}

#[test]
fn the_merge_rules_match_their_golden() {
    let mut transcript = String::new();

    transcript.push_str("# a whole class list on a component is a prop, not a forwarded entry\n");
    transcript.push_str(&transcript_of(view! { Root(class = "w-full") }));

    transcript.push_str("\n# class and style toggles compose, and the caller is last\n");
    transcript.push_str(&transcript_of(
        view! { Root(class:busy = true, style:gap = "2rem") },
    ));

    transcript.push_str("\n# the caller's accessibility properties win\n");
    transcript.push_str(&transcript_of(
        view! { Root(a11y:role = Role::Link, a11y:label = "caller") },
    ));

    transcript.push_str("\n# a caller who named no role does not take the component's away\n");
    transcript.push_str(&transcript_of(view! { Root(a11y:label = "caller") }));

    transcript.push_str("\n# listeners accumulate, the component's first\n");
    transcript.push_str(&transcript_of(view! { Root(on:click:capture = |_| {}) }));

    transcript.push_str("\n# attributes and properties are last-write-wins, caller last\n");
    transcript.push_str(&transcript_of(view! {
        Root(attr:data-part = "caller", prop:value = PropValue::Text("caller".into()))
    }));

    transcript.push_str("\n# a state a view may assert, and one it defines itself\n");
    transcript.push_str(&transcript_of(
        view! { Root(state:disabled, custom_state:selected = true) },
    ));

    transcript.push_str("\n# a forwarded bundle is replayed where the spread was written\n");
    let forwarded = Attrs::new()
        .class_toggle(ClassName::new("forwarded"), true)
        .attribute(AttrName::new("data-source"), Some("bundle".to_owned()));
    transcript.push_str(&transcript_of(view! {
        Root(class:before = true, {..forwarded}, class:after = true)
    }));

    let golden = include_str!("goldens/view/attrs_spread_merge.txt");
    assert_eq!(
        transcript, golden,
        "the merge transcript changed; the golden is tests/goldens/view/attrs_spread_merge.txt"
    );
}

/// `state:` names a closed set, and the macro keeps its own copy of that set because a proc-macro
/// crate cannot name the vocabulary the constants come from. Two homes for one concept drift, so
/// this asserts them equal from the outside: every name the grammar accepts is lowered, the bits
/// they lower to are unioned, and the union is compared against the vocabulary's own answer.
#[test]
fn the_states_a_view_may_assert_are_exactly_the_ones_the_vocabulary_allows() {
    let _transcript = transcript_of(view! {
        Root(
            state:checked,
            state:disabled,
            state:indeterminate,
            state:invalid,
            state:open,
            state:placeholder_shown,
            state:read_only,
            state:required
        )
    });
    assert_eq!(
        STATES.with(|states| states.get()),
        UiState::AUTHOR_SETTABLE,
        "the grammar's `state:` names and `UiState::AUTHOR_SETTABLE` have drifted apart"
    );
}
