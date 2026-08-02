//! Two transcripts, checked in.
//!
//! A golden is worth having here for the reason it is worth having anywhere: the interesting
//! failures are the ones nobody thought to assert on. "The variant class is written twice per
//! change", "the dialog traps focus before the surface exists", "closing it leaves the trap
//! installed" are all invisible to an assertion about the final tree and all obvious in a diff.

use std::path::Path;

use zgui_elements::{control, surface};
use zgui_interned::ClassName;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_testkit_view::Window;
use zgui_view::{Anchor, IntoView, Show, View, events};

/// Where the goldens live.
fn golden(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens/view")
        .join(name)
}

#[test]
fn button_variants() {
    let window = Window::open();
    let danger = window.scope.with(|| RwSignal::new(false));

    let mut built = window.scope.with(|| {
        control()
            .class("button")
            .class_toggle(ClassName::new("danger"), move || danger.get())
            .on(events::CLICK, move |_| danger.update(|on| *on = !*on))
            .child("Save")
            .into_view()
            .build(&mut window.cx.cx())
    });
    built.mount(&window.dom_handle, window.root, None);
    let button = built.first_node().expect("the view produced a node");
    window.place(button, 0.0, 0.0, 80.0, 24.0);
    window.frame();

    // Two presses, so the transcript shows the variant going on and coming off again — a binding
    // that wrote its class on every frame rather than on every change looks identical after one.
    window.click(10.0, 10.0);
    window.frame();
    window.click(10.0, 10.0);
    window.frame();

    window
        .transcript
        .assert_matches(golden("button_variants.txt"));
}

#[test]
fn dialog_open_close() {
    let window = Window::open();
    let open = window.scope.with(|| RwSignal::new(false));

    let mut built = window.scope.with(|| {
        Show::new(
            move || open.get(),
            move || {
                zgui_view::AnyView::new(
                    surface()
                        .class("dialog")
                        .child(control().class("close").child("Close")),
                )
            },
        )
        .into_view()
        .build(&mut window.cx.cx())
    });
    built.mount(&window.dom_handle, window.root, None);
    window.frame();
    window.transcript.clear();

    open.set(true);
    window.frame();
    open.set(false);
    window.frame();

    window
        .transcript
        .assert_matches(golden("dialog_open_close.txt"));
}
