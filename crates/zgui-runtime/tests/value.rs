//! What a field tells the view layer when its value changes, through the whole loop.
//!
//! Typing into the framework's own field is tested next door, against the document. This is the
//! other half, and it is the half a component library lives on: a view never reads the document,
//! so a field whose text changes and announces nothing is a field nothing can be bound to. That
//! failure is completely silent — the letters appear, the caret moves, every editing test passes,
//! and no `value` signal anywhere ever updates.
//!
//! So every assertion here is about what a listener registered with
//! [`events::INPUT`](zgui_view::events::INPUT) and [`events::CHANGE`](zgui_view::events::CHANGE)
//! actually received, and every event is a platform event handed to a real window.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, EventCx, IntoView, NodeRef, View, ViewHost, events};
use zgui_vocab::{
    EventKind, ImeEvent, Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey,
    Timestamp, ValueChange,
};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px }
                   control { display: block; width: 100px; height: 24px }";

/// One value change as a listener saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    /// Which event it arrived as, as the listener that ran was registered for.
    event: EventKind,
    /// What the payload itself says the change is.
    ///
    /// Asserted beside the event rather than instead of it: a value change delivered as the wrong
    /// event reaches listeners registered for the other one, and a payload assertion alone would
    /// pass while every `on:change` in an application fired on every keystroke.
    kind: ValueChange,
    /// The value it carried.
    value: String,
    /// The selection it carried.
    selection: core::ops::Range<usize>,
    /// Which element the listener that saw it was on.
    at: &'static str,
}

/// A press of a character key.
fn letter(text: &str) -> KeyEvent {
    KeyEvent {
        key: Key::Character(text.into()),
        key_without_modifiers: Key::Character(text.into()),
        physical: PhysicalKey::Code(KeyCode::KeyA),
        location: zgui_vocab::KeyLocation::Standard,
        repeat: false,
    }
}

/// One scripted window holding a field, a listener on it, and a listener above it.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The field.
    field: NodeRef,
    /// Every value change either listener saw, in order.
    seen: Rc<RefCell<Vec<Seen>>>,
}

impl Script {
    /// Delivers one surface event and lets the frames it produced settle.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    /// Presses one key.
    fn press(&mut self, event: KeyEvent) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event,
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// Presses a named key.
    fn press_named(&mut self, key: NamedKey, code: KeyCode) {
        self.press(KeyEvent::named(key, PhysicalKey::Code(code)));
    }

    /// Delivers one step of a composition.
    fn ime(&mut self, event: ImeEvent) {
        self.deliver(SurfaceEvent::Ime(event));
    }

    /// Everything either listener saw since it was last drained.
    fn drain(&self) -> Vec<Seen> {
        core::mem::take(&mut self.seen.borrow_mut())
    }

    /// Only what the listener on the field itself saw.
    fn drain_on_field(&self) -> Vec<Seen> {
        self.drain()
            .into_iter()
            .filter(|seen| seen.at == "field")
            .collect()
    }

    /// Which node holds focus.
    fn focused(&self) -> Option<zgui_view::NodeId> {
        use zgui_reactive::prelude::GetUntracked;
        self.harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
    }
}

/// A window holding a field with `content` in it, and a control after it to tab to.
///
/// `refuse` is the one character the field's own key handler takes responsibility for, which is
/// what an application writing a numeric-only field does; every other key falls through to the
/// framework. One character rather than all of them, so that a test about a refused key can press
/// an accepted one straight afterwards and prove that the announcing still works — a fixture that
/// refused everything would let a window announcing nothing at all pass.
fn scripted(content: &'static str, refuse: &'static str) -> Script {
    let field = NodeRef::new();
    let seen: Rc<RefCell<Vec<Seen>>> = Rc::new(RefCell::new(Vec::new()));

    /// Appends whatever a value listener was handed.
    fn record(
        seen: &Rc<RefCell<Vec<Seen>>>,
        at: &'static str,
    ) -> impl Fn(&mut EventCx<'_, events::Input>) + use<> {
        let seen = Rc::clone(seen);
        move |cx: &mut EventCx<'_, events::Input>| {
            let payload = cx.payload().as_value().expect("a value payload").clone();
            seen.borrow_mut().push(Seen {
                event: cx.kind,
                kind: payload.kind,
                value: payload.value.to_string(),
                selection: payload.selection,
                at,
            });
        }
    }

    /// The same, for the settled event, whose payload is the same type under another name.
    fn record_change(
        seen: &Rc<RefCell<Vec<Seen>>>,
        at: &'static str,
    ) -> impl Fn(&mut EventCx<'_, events::Change>) + use<> {
        let seen = Rc::clone(seen);
        move |cx: &mut EventCx<'_, events::Change>| {
            let payload = cx.payload().as_value().expect("a value payload").clone();
            seen.borrow_mut().push(Seen {
                event: cx.kind,
                kind: payload.kind,
                value: payload.value.to_string(),
                selection: payload.selection,
                at,
            });
        }
    }

    let recorded = Rc::clone(&seen);
    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                // On the ancestor, because a value change that only ever reached the field itself
                // would make every wrapper — a form, a labelled field, a search box — have to
                // reach inside the component it wraps.
                .on(events::INPUT, record(&recorded, "root"))
                .child(
                    zgui_elements::field()
                        .node_ref(field)
                        .on(events::INPUT, record(&recorded, "field"))
                        .on(events::CHANGE, record_change(&recorded, "field"))
                        .on(
                            events::KEY_DOWN,
                            move |cx: &mut EventCx<'_, events::KeyDown>| {
                                let typed = cx.payload().as_key().and_then(|key| key.key.as_str());
                                if !refuse.is_empty() && typed == Some(refuse) {
                                    cx.prevent_default();
                                }
                            },
                        )
                        .child(content),
                )
                .child(zgui_elements::control())
                .into_view()
                .build(cx),
        )
    });

    let mut script = Script {
        harness,
        field,
        seen,
    };
    script.harness.settle(8);
    // Tab into the window: the field is the first focusable thing in it.
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script.focused(),
        script.field.get_untracked(),
        "the fixture never got as far as focusing the field"
    );
    script.drain();
    script
}

/// The provisional and settled changes seen at the field, as `(value, selection)` pairs.
fn inputs(seen: Vec<Seen>) -> Vec<(String, core::ops::Range<usize>)> {
    seen.into_iter()
        .filter(|seen| seen.kind == ValueChange::Input)
        .map(|seen| (seen.value, seen.selection))
        .collect()
}

#[test]
fn typing_into_a_field_announces_the_whole_new_value_and_where_the_caret_landed() {
    // The one that catches the seam being unwired: the field types perfectly and a listener that
    // was registered for the change hears nothing at all.
    let mut script = scripted("ab", "");
    script.press(letter("z"));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("zab".to_owned(), 1..1)],
        "typing announced nothing, so nothing above the field can learn what it holds"
    );

    script.press(letter("y"));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("zyab".to_owned(), 2..2)],
        "the value is the whole text, not the piece that was inserted"
    );

    script.press_named(NamedKey::Backspace, KeyCode::Backspace);
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("zab".to_owned(), 1..1)],
        "a deletion is a value change like any other"
    );
}

#[test]
fn a_value_change_travels_the_ordinary_path_and_reaches_an_ancestor() {
    let mut script = scripted("ab", "");
    script.press(letter("z"));
    let seen = script.drain();
    assert_eq!(
        seen.iter().map(|seen| seen.at).collect::<Vec<_>>(),
        vec!["field", "root"],
        "the change did not bubble, so a wrapper cannot hear its own field"
    );
    assert!(seen.iter().all(|seen| seen.value == "zab"));
}

#[test]
fn moving_the_caret_announces_no_value_change_at_all() {
    // The vacuous version of this test is one that only ever presses arrows: a window that
    // announced nothing whatsoever — the seam left unwired, which is the defect this file is about
    // — passes it exactly as a correct one does. So the same window types afterwards and the
    // announcement is required to arrive, which is what makes the silence above mean something.
    let mut script = scripted("abc", "");
    for key in [
        NamedKey::ArrowRight,
        NamedKey::ArrowRight,
        NamedKey::ArrowLeft,
        NamedKey::Home,
        NamedKey::End,
    ] {
        script.press_named(key, KeyCode::ArrowRight);
    }
    assert_eq!(
        script.drain(),
        Vec::new(),
        "moving the caret was announced as the value changing"
    );
    assert_eq!(script.field.selection(), Some(3..3), "the caret did move");

    script.press(letter("z"));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("abcz".to_owned(), 4..4)],
        "this window announces nothing at all, so the silence above proved nothing"
    );
}

#[test]
fn a_handler_that_takes_responsibility_for_the_key_announces_nothing() {
    // Editing is a default action, and so is announcing what it did: a field whose handler refused
    // the key has no new value, and reporting one would drive a bound signal from text that is not
    // on the screen.
    //
    // The handler refuses one character and lets the rest through, so the accepted key that
    // follows has to be announced — and the value it announces is the proof that the refused key
    // typed nothing either. A fixture that refused every key would let a window that announces
    // nothing whatsoever pass this, which is the very defect being tested for.
    let mut script = scripted("ab", "z");
    script.press(letter("z"));
    assert_eq!(script.drain(), Vec::new());

    script.press(letter("y"));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("yab".to_owned(), 1..1)],
        "the accepted key was not announced, so the refused one proved nothing"
    );
    assert_eq!(
        script.field.selection(),
        Some(1..1),
        "the refused key moved the caret, so it was typed after all"
    );
}

#[test]
fn a_composition_announces_its_provisional_text_and_then_what_was_committed() {
    let mut script = scripted("ab", "");
    script.press_named(NamedKey::End, KeyCode::End);
    script.drain();

    script.ime(ImeEvent::Enabled);
    script.ime(ImeEvent::Preedit {
        text: "にほん".into(),
        cursor: Some(9..9),
    });
    assert_eq!(
        inputs(script.drain_on_field())
            .into_iter()
            .map(|(value, _)| value)
            .collect::<Vec<_>>(),
        vec!["abにほん".to_owned()],
        "provisional text is what the field holds right now, and was not announced"
    );

    script.ime(ImeEvent::Commit("日本".into()));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("ab日本".to_owned(), 8..8)],
        "the commit was not announced, so a bound value keeps the provisional text for ever"
    );
}

#[test]
fn an_abandoned_composition_announces_the_text_it_left_behind() {
    // Both winit backends end a composition that produced nothing with `Preedit("")` and nothing
    // behind it — no commit, no dismissal, ever. The provisional text is taken back out of the
    // document, so the value really did change again, and a bound signal that was never told is
    // left showing text the field no longer holds.
    let mut script = scripted("ab", "");
    script.ime(ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    assert_eq!(
        inputs(script.drain_on_field())
            .into_iter()
            .map(|(value, _)| value)
            .collect::<Vec<_>>(),
        vec!["にab".to_owned()]
    );

    script.ime(ImeEvent::Preedit {
        text: String::new().into(),
        cursor: None,
    });
    assert_eq!(
        inputs(script.drain_on_field())
            .into_iter()
            .map(|(value, _)| value)
            .collect::<Vec<_>>(),
        vec!["ab".to_owned()],
        "the abandoned composition was retracted from the document and announced to nobody"
    );

    // And the window is still a window: the next key types and is announced.
    script.press(letter("z"));
    assert_eq!(
        inputs(script.drain_on_field()),
        vec![("zab".to_owned(), 1..1)]
    );
}

#[test]
fn leaving_a_field_that_was_typed_into_settles_its_value_exactly_once() {
    // The other half of the pair. A live search wants every keystroke; a form that validates on the
    // server wants the value once the user has stopped, and until this fires the only way to know
    // they stopped is to guess.
    let mut script = scripted("ab", "");
    script.press(letter("z"));
    script.drain();

    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_ne!(script.focused(), script.field.get_untracked());
    assert_eq!(
        script.drain(),
        vec![Seen {
            event: EventKind::Change,
            kind: ValueChange::Committed,
            value: "zab".to_owned(),
            selection: 3..3,
            at: "field",
        }],
        "leaving the field announced nothing settled"
    );

    // Back into it and out again without typing. Nothing changed, so nothing settled: a form that
    // revalidated every time the user tabbed past a field would complain about text nobody touched.
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script
            .drain()
            .into_iter()
            .filter(|seen| seen.kind == ValueChange::Committed)
            .count(),
        0,
        "a field that was only looked at settled a second time"
    );
}
