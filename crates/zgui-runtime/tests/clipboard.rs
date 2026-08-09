//! The clipboard an application reaches itself, through the whole loop.
//!
//! Every assertion here goes from a component calling [`use_clipboard`] to the platform's own
//! clipboard and back. That is the point: the queue, the drain where the platform context is, and —
//! for a read — the answer arriving as a wake and reaching the callback that started it are four
//! separate steps, and no crate proves the sequence on its own.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::{
    ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions, PlatformCx,
};
use zgui_platform_headless::Harness;
use zgui_reactive::prelude::*;
use zgui_runtime::Runtime;
use zgui_runtime::clipboard::{Clipboards, use_clipboard};
use zgui_view::{BuildCx, IntoView, View};

/// A root that lays out and draws nothing in particular.
const CSS: &str = "root { display: block; width: 200px; height: 100px }";

/// An application whose view runs `body` once, with the clipboards resolved from its own scope.
fn app_running(body: impl FnOnce(Clipboards) + 'static) -> Harness<Runtime> {
    let mut body = Some(body);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        if let Some(body) = body.take() {
            body(use_clipboard());
        }
        Box::new(zgui_elements::column().class("root").into_view().build(cx))
    });
    harness.settle(8);
    harness
}

/// What is on one of the platform's clipboards.
fn held(harness: &Harness<Runtime>, kind: ClipboardKind) -> Option<String> {
    harness
        .platform()
        .clipboard()
        .read_blocking(kind, ClipboardFormat::Text)
        .ok()
        .and_then(|data| data.as_text().map(str::to_owned))
}

/// Puts `text` on one of the platform's clipboards, before the application asks for it.
fn preload(harness: &Harness<Runtime>, kind: ClipboardKind, text: &str) {
    harness
        .platform()
        .clipboard()
        .write(
            kind,
            ClipboardData::from(text),
            ClipboardWriteOptions::default(),
        )
        .expect("an in-memory clipboard takes text");
}

#[test]
fn a_component_can_put_text_on_the_clipboard() {
    let harness = app_running(|clipboards| {
        clipboards.set_text(ClipboardKind::Standard, "copied");
    });

    assert_eq!(
        held(&harness, ClipboardKind::Standard).as_deref(),
        Some("copied")
    );
}

#[test]
fn the_selection_and_the_clipboard_are_separate() {
    let harness = app_running(|clipboards| {
        clipboards.set_text(ClipboardKind::Primary, "selected");
    });

    assert_eq!(
        held(&harness, ClipboardKind::Primary).as_deref(),
        Some("selected"),
        "copy-on-select reaches the selection"
    );
    assert_eq!(
        held(&harness, ClipboardKind::Standard),
        None,
        "and leaves what the user last copied alone"
    );
}

#[test]
fn a_read_answers_the_callback_that_started_it() {
    let seen: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));
    let answer = Rc::clone(&seen);

    let mut harness = support::app_with_text(CSS, {
        let mut once = Some(());
        move |cx: &mut BuildCx<'_>| {
            if once.take().is_some() {
                let answer = Rc::clone(&answer);
                use_clipboard().read_text(ClipboardKind::Standard, move |text| {
                    *answer.borrow_mut() = Some(text);
                });
            }
            Box::new(zgui_elements::column().class("root").into_view().build(cx))
        }
    });
    // Loaded before the first frame runs, so the read finds it.
    preload(&harness, ClipboardKind::Standard, "pasted");
    harness.settle(8);

    assert_eq!(
        *seen.borrow(),
        Some(Some("pasted".to_owned())),
        "the answer came back through the wake and reached the callback"
    );
}

#[test]
fn a_read_of_an_empty_clipboard_answers_with_nothing() {
    let seen: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));
    let answer = Rc::clone(&seen);

    let _harness = app_running(move |clipboards| {
        clipboards.read_text(ClipboardKind::Standard, move |text| {
            *answer.borrow_mut() = Some(text);
        });
    });

    assert_eq!(
        *seen.borrow(),
        Some(None),
        "an empty clipboard answers, and answers with nothing"
    );
}

#[test]
fn a_read_reaches_the_signal_it_was_asked_into() {
    type Held =
        Rc<RefCell<Option<zgui_reactive::Signal<Option<String>, zgui_reactive::LocalStorage>>>>;
    let signal: Held = Rc::new(RefCell::new(None));
    let captured = Rc::clone(&signal);

    let mut harness = support::app_with_text(CSS, {
        let mut once = Some(());
        move |cx: &mut BuildCx<'_>| {
            if once.take().is_some() {
                *captured.borrow_mut() =
                    Some(use_clipboard().read_text_signal(ClipboardKind::Standard));
            }
            Box::new(zgui_elements::column().class("root").into_view().build(cx))
        }
    });
    preload(&harness, ClipboardKind::Standard, "reactive");
    harness.settle(16);

    let held = signal.borrow().expect("the view asked for a read").get();
    assert_eq!(
        held.as_deref(),
        Some("reactive"),
        "the signal held the answer once it arrived"
    );
}

#[test]
fn clearing_empties_the_clipboard() {
    let harness = app_running(|clipboards| {
        clipboards.set_text(ClipboardKind::Standard, "copied");
        clipboards.clear(ClipboardKind::Standard);
    });

    assert_eq!(held(&harness, ClipboardKind::Standard), None);
}
