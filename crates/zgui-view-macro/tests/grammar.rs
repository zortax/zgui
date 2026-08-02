//! A view built and mounted, form by form.
//!
//! The parser's own tests read what a view lowers to; these read what it *is* once it has been
//! mounted, which is the only place a prop that was dropped or a child that was reparented shows
//! up as a difference in what the interface says.

// The root a view expansion names its crates through; an application gets it from the umbrella
// crate instead.
extern crate zgui_view as zgui;

mod support;

use support::Harness;
use zgui_reactive::{RwSignal, flush};
use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

/// A component with a required prop, a defaulted one and children.
#[component]
fn Label(
    /// What it says.
    #[prop(into)]
    text: String,
    /// How many times it is repeated.
    #[prop(default = 1)]
    times: usize,
    /// Said after the text.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    let repeated = text.repeat(times);
    let rest = children.map(Children::into_view_once);
    view! { {repeated}{rest} }
}

/// The text one view mounts to, in a harness of its own.
fn mounted<V: IntoView>(view: V) -> String {
    let harness = Harness::new();
    let _state = harness.mount(view);
    harness.text()
}

#[test]
fn a_prop_written_in_the_attribute_list_reaches_the_component() {
    assert_eq!(mounted(view! { Label(text = "a", times = 3) }), "aaa");
    assert_eq!(mounted(view! { Label(text = "a") }), "a");
}

#[test]
fn children_written_in_the_block_are_the_component_s_children() {
    assert_eq!(mounted(view! { Label(text = "a") { "!" } }), "a!");
}

#[test]
fn nodes_written_side_by_side_are_a_fragment() {
    assert_eq!(mounted(view! { Label(text = "a") Label(text = "b") }), "ab");
}

/// An expression in an attribute value is delimited by the attribute list around it, so a
/// comparison is a comparison and needs no braces.
#[test]
fn a_value_may_compare_two_numbers() {
    let count = 3usize;
    assert_eq!(
        mounted(view! { Label(text = "a", times = if count > 2 { 2 } else { 1 }) }),
        "aa"
    );
}

/// The keywords are sugar for the components, so what they lower to is what re-runs: a list
/// rebuilds its rows and a conditional swaps its branch when the signal its head reads changes.
/// The head is a closure by token for exactly this reason, and this is the assertion that the
/// closure the parser copied is the one the component calls again.
#[test]
fn control_flow_re_reads_what_its_head_reads() {
    let harness = Harness::new();
    let items = harness.window.with(|| RwSignal::new(vec![1usize, 2]));
    let _state = harness.mount(view! {
        for item in move || items.get(), key = |item: &usize| *item {
            {item.to_string()}
        }
        if move || items.get().is_empty() {
            "none"
        } else {
            "some"
        }
    });
    assert_eq!(harness.text(), "12some");

    items.set(vec![3]);
    flush();
    assert_eq!(harness.text(), "3some");

    items.set(Vec::new());
    flush();
    assert_eq!(harness.text(), "none");
}

/// A head written without `move` is accepted and copied as it was written.
///
/// What the keyword requires is that the head *be* a closure, which is a rule about the first
/// token; whether that closure captures by reference or by value is Rust's question and the parser
/// does not answer it. So a list over a collection that borrows nothing is written without `move`,
/// and the head reaches the component exactly as it was typed.
///
/// It does not follow that `move` is optional everywhere. A head that reads a signal held by the
/// enclosing function outlives that function once the component stores it, so the borrow checker
/// asks for `move` there and says so — which is the ordinary Rust error, arriving in the ordinary
/// place, rather than anything this grammar invented.
#[test]
fn a_head_written_without_move_is_copied_as_it_was_written() {
    let harness = Harness::new();
    let _state = harness.mount(view! {
        for item in || [1usize, 2, 3], key = |item: &usize| *item {
            {item.to_string()}
        }
    });
    assert_eq!(harness.text(), "123");
}
