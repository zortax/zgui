//! Keys on a window in which nothing has focus.
//!
//! A key is delivered along the path to whatever holds focus. When nothing does, that path is the
//! document's root and nothing else — so a listener anywhere below the root hears nothing at all,
//! which is the state a window is in the moment it opens and every moment after focus is dropped.
//! An application-wide chord written as an ordinary handler is therefore dead exactly when it is
//! reached for.
//!
//! Both halves are asserted here, and the second is the reason the first is shaped the way it is.
//! Widening the unfocused path so that deep handlers hear a key would repair the chord and break
//! everything else: nothing in the tree marks a handler as wanting the keyboard when nobody has
//! it, so a wider path is every key handler in the document, and a list's type-ahead would start
//! consuming keys aimed at nothing. What hears an unfocused key is registered instead.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::SurfaceEvent;
use zgui_reactive::prelude::GetUntracked;
use zgui_view::{NodeRef, ViewHost, WindowShortcut};
use zgui_vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// Two elements under the root, both deep enough to be off an unfocused key's path.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
control { display: block; width: 100px; height: 24px }
";

/// One window, the two counters, and the registration that decides which of them moves.
struct Run {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// How many keys the registered element heard.
    shortcut: Rc<Cell<usize>>,
    /// How many keys the ordinary deep handler heard.
    ordinary: Rc<Cell<usize>>,
    /// The registration, dropped by hand in one of the tests.
    guard: Option<WindowShortcut>,
}

impl Run {
    /// A focused window with nothing in it holding focus, and the registration in place.
    fn opened() -> Self {
        let shortcut = Rc::new(Cell::new(0));
        let ordinary = Rc::new(Cell::new(0));
        let anchor = NodeRef::new();

        let heard = Rc::clone(&shortcut);
        let missed = Rc::clone(&ordinary);
        let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
            use zgui_view::{IntoView, View};
            let heard = Rc::clone(&heard);
            let missed = Rc::clone(&missed);
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::column()
                            .node_ref(anchor)
                            .on(
                                zgui_view::events::KEY_DOWN,
                                move |_: &mut zgui_view::EventCx<'_, _>| {
                                    heard.set(heard.get() + 1);
                                },
                            )
                            .child(zgui_elements::control()),
                    )
                    .child(zgui_elements::column().on(
                        zgui_view::events::KEY_DOWN,
                        move |_: &mut zgui_view::EventCx<'_, _>| {
                            missed.set(missed.get() + 1);
                        },
                    ))
                    .into_view()
                    .build(cx),
            )
        });
        harness.deliver_to_first(SurfaceEvent::Focused(true));
        harness.settle(16);

        let guard = anchor.window_shortcut();
        assert!(
            guard.is_some(),
            "the element the registration names was never bound"
        );
        Self {
            harness,
            shortcut,
            ordinary,
            guard,
        }
    }

    /// Whether anything in the window holds focus.
    fn anything_focused(&self) -> bool {
        self.harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
            .is_some()
    }

    /// Presses <kbd>F12</kbd> and lets the frames it produced settle.
    fn press(&mut self) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::F12, PhysicalKey::Code(KeyCode::F12)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(8);
    }

    /// Tabs into the window, so that something holds focus.
    fn tab(&mut self) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(16);
    }
}

/// The registration hears a key nothing has focus for; the ordinary handler beside it does not.
#[test]
fn a_window_shortcut_hears_an_unfocused_key_and_a_deep_listener_does_not() {
    let mut run = Run::opened();
    assert!(
        !run.anything_focused(),
        "the window focused something before the key was pressed, which is the one condition that \
         makes this pass for the wrong reason"
    );

    run.press();

    assert_eq!(
        run.shortcut.get(),
        1,
        "the registered element did not hear a key nothing had focus for"
    );
    assert_eq!(
        run.ordinary.get(),
        0,
        "an ordinary deep listener heard a key nothing had focus for, which is what a wider \
         unfocused path would have done to every key handler in the document"
    );
}

/// Dropping the guard puts the element back to hearing nothing.
#[test]
fn dropping_the_registration_stops_the_delivery() {
    let mut run = Run::opened();
    run.press();
    assert_eq!(run.shortcut.get(), 1);

    run.guard.take();
    run.press();
    assert_eq!(
        run.shortcut.get(),
        1,
        "the element went on hearing unfocused keys after its registration was dropped"
    );
}

/// A registration adds nothing to the route once something does hold focus.
///
/// The delivery a focused key already gets is the path down to whatever holds it, and the
/// registered element is on that path or it is not. Appending it as well would run its handler
/// twice on every key typed into a control inside it.
#[test]
fn a_focused_key_reaches_the_registration_exactly_once() {
    let mut run = Run::opened();
    run.tab();
    assert!(run.anything_focused(), "the tab focused nothing");
    // The tab itself was an unfocused key and the registration heard it, which is the point of the
    // registration. What this test is about is the key *after* it.
    run.shortcut.set(0);

    run.press();
    assert_eq!(
        run.shortcut.get(),
        1,
        "the registered element heard the same focused key more than once"
    );
}
