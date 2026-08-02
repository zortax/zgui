//! A handler annotated for one event, attached to another.
//! 
//! The annotation is what makes this a wrong *signature* rather than an inference failure: the
//! closure says which payload it reads, and the element says which event it is listening for,
//! and the two disagree.

extern crate zgui_view as zgui;

use zgui_view::events::{Click, KeyDown};
use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let _ = view! { Thing(on:click = move |ev: &mut EventCx<'_, KeyDown>| { let _ = ev; }) };
    let _ = view! { Thing(on:key_down = move |ev: &mut EventCx<'_, Click>| { let _ = ev; }) };
}
