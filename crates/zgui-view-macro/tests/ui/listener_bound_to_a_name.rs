//! A handler written apart from the element it is attached to.
//!
//! Both spellings must work: the closure written where it is attached, and the closure given a
//! name first. The named one is built with `handler`, which supplies the event at the binding —
//! without it the closure's argument type is settled before anything says what it is for, and the
//! attachment is refused with a message about `Fn` not being general enough.

extern crate zgui_view as zgui;

use zgui_reactive::{RwSignal, install};
use zgui_reactive::prelude::{Get, Set};
use zgui_view::events;
use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    install().ok();
    let picked = RwSignal::new(0_usize);

    // Named first, with no annotation on the argument at all.
    let pick = handler(events::CLICK, move |_| picked.set(1));
    let _ = view! { Thing(on:click = pick) };

    // Named, reading the payload the event carries.
    let moved = handler(events::POINTER_MOVE, move |ev| {
        let _ = ev.position;
        let _ = picked.get();
    });
    let _ = view! { Thing(on:pointer_move = moved) };

    // And the inline form, which must keep working.
    let _ = view! { Thing(on:click = move |_| picked.set(2)) };
}
