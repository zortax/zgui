//! A field whose text an application owns, and a window that stops being typed into.
//!
//! Both halves are seams that were declared and never joined, and both fail in the same silent
//! way: everything compiles, the field types perfectly, and the application's own value has no
//! effect on what is on the screen.
//!
//! A **controlled** field is the ordinary case in a reactive framework. The application holds the
//! text in a signal, the field announces what the user did, the application writes its signal, and
//! the signal drives the field back. Every assertion here is written over that whole loop, driven
//! by real key events through a real window, and read out of the document the window actually
//! shows — never out of the signal, which is the one place a completely unwired field looks right.
//!
//! The loop closes on itself, and that is what makes the caret the interesting part. An application
//! that transforms what it was told writes back a *different* string on the very keystroke it was
//! told about, so the field is re-loaded while the person is still typing into it; an application
//! that transforms nothing writes back the string that is already there, and the field must then do
//! nothing at all rather than rebuild itself. Both are tested, and both are tested by typing again
//! afterwards and asking where the letter landed.
//!
//! The other half is the **surface** losing the keyboard. The window system takes an input method
//! away with the focus and never says another word about it, so a composition left open is left
//! open for ever — and the field it is in refuses every key from then on, because a model that
//! believes it is composing must refuse keys.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::prelude::{Get, GetUntracked, Set};
use zgui_reactive::{RenderEffect, RwSignal};
use zgui_view::{BuildCx, EventCx, IntoView, NodeRef, View, ViewHost, events};
use zgui_vocab::{
    EventKind, ImeEvent, Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey,
    PointerAction, PointerEvent, Timestamp, ValueChange,
};

/// The sheet the fixture is styled by.
///
/// The `:active` rule is what makes a press visible as a *computed* style: the state the press
/// wrote is what the cascade matches on, and the colour that comes out of it is what is painted.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px;
                           background-color: rgb(10, 20, 30) }
                   field:active { background-color: rgb(200, 100, 50) }
                   control { display: block; width: 100px; height: 24px }";

/// The background the field is painted in while nothing is holding it down.
const CALM: [u8; 3] = [10, 20, 30];

/// The background the field is painted in while it is being pressed.
const PRESSED: [u8; 3] = [200, 100, 50];

/// One value change a listener saw.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    /// Which event it arrived as.
    event: EventKind,
    /// What the payload says the change is.
    kind: ValueChange,
    /// The value it carried.
    value: String,
}

/// What the application does with each value the field announces.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    /// Writes it back exactly as it arrived, which is the plain controlled field.
    Verbatim,
    /// Writes back a transformed version of it, which re-loads the field mid-keystroke.
    Shouting,
}

/// One scripted window holding a field driven by a signal.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The field.
    field: NodeRef,
    /// The signal the application owns the text in.
    value: RwSignal<String, zgui_reactive::LocalStorage>,
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
    fn press(&mut self, event: KeyEvent, modifiers: Modifiers) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event,
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// Types one letter.
    fn type_letter(&mut self, text: &str) {
        self.press(letter(text), Modifiers::NONE);
    }

    /// Presses a named key.
    fn press_named(&mut self, key: NamedKey, code: KeyCode) {
        self.press(
            KeyEvent::named(key, PhysicalKey::Code(code)),
            Modifiers::NONE,
        );
    }

    /// Delivers one step of a composition.
    fn ime(&mut self, event: ImeEvent) {
        self.deliver(SurfaceEvent::Ime(event));
    }

    /// Tells the window whether its surface has the keyboard.
    fn window_focus(&mut self, focused: bool) {
        self.deliver(SurfaceEvent::Focused(focused));
    }

    /// Does something with the pointer at a point in the window, in CSS pixels.
    fn pointer(&mut self, action: PointerAction, x: f32, y: f32) {
        self.deliver(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// The text the *document* holds under the field, which is what the window shows.
    ///
    /// Read out of the tree rather than out of the signal on purpose: the signal is written by the
    /// application itself and says nothing whatever about whether the field followed it.
    fn shown(&self) -> String {
        let window = &self.harness.app().windows()[0];
        let document = window.document().borrow();
        let key = zgui_view_dom::id::to_document(self.field.get_untracked().expect("mounted"))
            .expect("a document node");
        let index = document.store().index_of(key).expect("still in the tree");
        let mut text = String::new();
        let mut child = document.store().core(index).first_child();
        let mut first = true;
        while let Some(node) = child {
            if document.store().core(node).kind() == zgui_dom::NodeKind::Text {
                if !first {
                    text.push('\n');
                }
                first = false;
                text.push_str(zgui_dom::text::text_of(document.store(), node).unwrap_or_default());
            }
            child = document.store().core(node).next_sibling();
        }
        text
    }

    /// Where the caret or the selection is in the field.
    fn caret(&self) -> Option<core::ops::Range<usize>> {
        self.field.selection()
    }

    /// The background colour the cascade computed for the field, as bytes.
    ///
    /// Read out of the style the box was laid out with — the cascade's own answer, after selector
    /// matching — rather than out of a state bit. A state bit is the *input* to the question this
    /// asks: whether the rule that names the state won, and what colour came out of it.
    fn field_background(&self) -> [u8; 3] {
        let window = &self.harness.app().windows()[0];
        let key = zgui_view_dom::id::to_document(self.field.get_untracked().expect("mounted"))
            .expect("a document node");
        let layout = window.layout().borrow();
        let box_key = *layout
            .boxes_of(key)
            .first()
            .expect("the field was laid out");
        let style = &layout.node(box_key).style;
        let color = zgui_paint::lower::background::of(style)
            .color
            .to_space(zgui_color::ColorSpace::Srgb);
        color
            .components()
            .map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    /// Everything either listener saw since it was last drained.
    fn drain(&self) -> Vec<Seen> {
        core::mem::take(&mut self.seen.borrow_mut())
    }

    /// Only the settled changes seen since the last drain.
    ///
    /// A settled value has to arrive as a `change` event *and* say so in its payload. Filtering on
    /// the payload alone would accept one delivered to the `input` listener, which is the listener
    /// a live search is on: a form that validates on the server would then never run, and every
    /// keystroke would look like the user had finished.
    fn drain_settled(&self) -> Vec<String> {
        self.drain()
            .into_iter()
            .filter(|seen| seen.kind == ValueChange::Committed)
            .map(|seen| {
                assert_eq!(
                    seen.event,
                    EventKind::Change,
                    "a settled value reached the wrong listener"
                );
                seen.value
            })
            .collect()
    }

    /// The moment the window next owes a frame at, if it owes one at all.
    ///
    /// In this fixture nothing animates, no timer is set and no resize is pending, so the only
    /// thing that can install one is the caret's own blink — which makes this the way to ask
    /// whether the window is still waking up to draw an insertion point.
    fn next_wake(&self) -> Option<std::time::Instant> {
        let now = self.harness.now();
        self.harness.app().windows()[0].merged_deadline(now)
    }

    /// What the surface was last told about text input.
    ///
    /// The outer option is whether it has ever been told; the inner one is what it was told, and
    /// `None` there is "no text is being typed here" — a thing that has to be said out loud,
    /// because an input method that was never told stays ready to compose into a window that has
    /// not got the keyboard.
    fn told_text_input(&self) -> Option<Option<zgui_platform::TextInput>> {
        self.harness.platform().offscreens()[0].last_text_input()
    }

    /// Which node holds focus in the document.
    fn focused(&self) -> Option<zgui_view::NodeId> {
        self.harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
    }
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

/// A window whose field is driven by a signal, exactly as an application drives one.
///
/// The three pieces of a controlled field, and nothing else: an effect that loads the signal into
/// the field, a listener that tells the application what the user did, and the application's own
/// decision about what to write back.
fn controlled(start: &str, owner: Owner) -> Script {
    let field = NodeRef::new();
    let value = RwSignal::new_local(start.to_owned());
    let seen: Rc<RefCell<Vec<Seen>>> = Rc::new(RefCell::new(Vec::new()));

    let recorded = Rc::clone(&seen);
    let content = start.to_owned();
    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        // The controlled binding itself. Held alive by the cleanup, exactly as a component holds
        // one: an effect whose handle is dropped stops running, and a field bound to a dead effect
        // follows its signal precisely once.
        let driving = RenderEffect::new(move |_| field.set_value(&value.get()));
        zgui_reactive::on_cleanup_local(move || drop(driving));

        let on_input = {
            let seen = Rc::clone(&recorded);
            move |cx: &mut EventCx<'_, events::Input>| {
                let payload = cx.payload().as_value().expect("a value payload").clone();
                seen.borrow_mut().push(Seen {
                    event: cx.kind,
                    kind: payload.kind,
                    value: payload.value.to_string(),
                });
                let written = match owner {
                    Owner::Verbatim => payload.value.to_string(),
                    Owner::Shouting => payload.value.to_uppercase(),
                };
                value.set(written);
            }
        };
        let on_change = {
            let seen = Rc::clone(&recorded);
            move |cx: &mut EventCx<'_, events::Change>| {
                let payload = cx.payload().as_value().expect("a value payload").clone();
                seen.borrow_mut().push(Seen {
                    event: cx.kind,
                    kind: payload.kind,
                    value: payload.value.to_string(),
                });
            }
        };

        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::field()
                        .node_ref(field)
                        .on(events::INPUT, on_input)
                        .on(events::CHANGE, on_change)
                        .child(content.clone()),
                )
                .child(zgui_elements::control())
                .into_view()
                .build(cx),
        )
    });

    let mut script = Script {
        harness,
        field,
        value,
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

#[test]
fn a_field_follows_the_signal_that_owns_it() {
    // The plainest form of the defect: an application sets its own value and the window shows the
    // old text for ever, because nothing joins a signal to an editing model.
    let mut script = controlled("", Owner::Verbatim);
    assert_eq!(script.shown(), "");

    script.value.set("hello".to_owned());
    script.harness.settle(8);
    assert_eq!(
        script.shown(),
        "hello",
        "the value the application wrote never reached the field"
    );

    // And it keeps following it, including all the way back to empty — a field that only ever grew
    // would pass an assertion about one write.
    script.value.set("goodbye".to_owned());
    script.harness.settle(8);
    assert_eq!(script.shown(), "goodbye");
    script.value.set(String::new());
    script.harness.settle(8);
    assert_eq!(script.shown(), "");

    assert_eq!(
        script.drain(),
        Vec::new(),
        "a value the application wrote was announced back to it, which is a loop"
    );
}

#[test]
fn typing_into_a_controlled_field_is_announced_and_the_transformed_echo_keeps_the_caret() {
    // The case a controlled field lives or dies on. The application upper-cases what it was told,
    // so every keystroke re-loads the field with text that is *not* what is there — while the
    // person is still typing into it. Where the next letter lands is the whole assertion: a load
    // that put the caret at either end of the text types the rest of the word in the wrong place,
    // and the text alone cannot tell the difference until the caret is somewhere other than the end.
    let mut script = controlled("", Owner::Shouting);

    script.type_letter("a");
    assert_eq!(
        script
            .drain()
            .into_iter()
            .map(|seen| seen.value)
            .collect::<Vec<_>>(),
        vec!["a".to_owned()],
        "the field announced nothing, so the application never learns what was typed"
    );
    assert_eq!(
        script.shown(),
        "A",
        "the application's transformed value never came back to the field"
    );
    assert_eq!(script.caret(), Some(1..1));

    script.type_letter("b");
    script.type_letter("c");
    assert_eq!(script.shown(), "ABC");
    assert_eq!(script.caret(), Some(3..3));
    assert_eq!(script.value.get_untracked(), "ABC");

    // Now the part the end of the text cannot answer for. The caret goes to the front, and the two
    // letters typed there have to appear there, in order.
    script.press_named(NamedKey::Home, KeyCode::Home);
    assert_eq!(script.caret(), Some(0..0));
    script.type_letter("x");
    assert_eq!(script.shown(), "XABC");
    assert_eq!(
        script.caret(),
        Some(1..1),
        "the echo moved the caret, so the rest of the word will be typed somewhere else"
    );
    script.type_letter("y");
    assert_eq!(
        script.shown(),
        "XYABC",
        "the second letter did not land after the first"
    );
}

#[test]
fn the_echo_of_a_value_a_field_already_holds_leaves_the_caret_and_the_history_alone() {
    // The plain controlled field: the application writes back exactly what it was told, so the
    // field is handed its own text on every keystroke. Doing anything at all with it is what
    // rebuilds the model — and the model is where the undo stack lives, so the visible symptom is
    // a field that types correctly and cannot undo a single letter.
    let mut script = controlled("", Owner::Verbatim);
    script.type_letter("a");
    script.type_letter("b");
    script.type_letter("c");
    assert_eq!(script.shown(), "abc");
    assert_eq!(script.caret(), Some(3..3));

    // A deliberate caret move ends one undo entry, so what follows it is a second one. Both have to
    // be there afterwards: an echo that rebuilt the model would leave the stack holding at most
    // whatever was typed since the last echo, which is nothing at all.
    script.press_named(NamedKey::Home, KeyCode::Home);
    script.type_letter("x");
    assert_eq!(script.shown(), "xabc");

    script.press(letter("z"), Modifiers::CONTROL);
    assert_eq!(
        script.shown(),
        "abc",
        "the echo of the field's own value threw away the undo stack"
    );
    assert_eq!(
        script.caret(),
        Some(0..0),
        "an undo put the caret back where the change it took out had begun"
    );

    script.press(letter("z"), Modifiers::CONTROL);
    assert_eq!(
        script.shown(),
        "",
        "the entry before the caret move was gone, so the echo cleared the stack after all"
    );
}

#[test]
fn the_window_losing_the_keyboard_settles_the_field_exactly_once() {
    // Leaving a field announces the value the user settled on, and it is the only moment a form has
    // to validate on. A window whose field is left by the whole window going away announced
    // nothing, so everything typed before an alt-tab was never committed to anything.
    let mut script = controlled("ab", Owner::Verbatim);
    script.type_letter("z");
    script.drain();

    script.window_focus(false);
    assert_eq!(
        script.drain_settled(),
        vec!["zab".to_owned()],
        "the window losing the keyboard settled nothing"
    );
    assert_eq!(
        script.focused(),
        script.field.get_untracked(),
        "the element lost focus too, so coming back puts the caret nowhere"
    );

    assert_eq!(
        script.next_wake(),
        None,
        "the caret is still being blinked behind whatever is in front of the window, \
         so the loop wakes to draw an insertion point nobody can see, twice a second, for ever"
    );

    // Re-stated, as a window system does. Settling twice is a form that validates twice.
    script.window_focus(false);
    assert_eq!(script.drain_settled(), Vec::<String>::new());

    // Back, and the field is a field again: it types, at the caret it was left at rather than at
    // either end, and it settles once more on what it now holds.
    script.window_focus(true);
    assert_eq!(
        script.caret(),
        Some(1..1),
        "the caret moved while nobody was there"
    );
    assert!(
        script.next_wake().is_some(),
        "the window came back and its caret never started blinking again"
    );
    script.type_letter("q");
    assert_eq!(
        script.shown(),
        "zqab",
        "the field stopped taking keys after the window came back"
    );
    script.drain();
    script.window_focus(false);
    assert_eq!(script.drain_settled(), vec!["zqab".to_owned()]);
}

#[test]
fn the_window_losing_the_keyboard_keeps_the_composed_text_and_ends_the_composition() {
    // The input method goes away with the focus and says nothing else — no commit, no dismissal.
    // What is on the screen has to stay: the person typed it and can see it. What must not stay is
    // the *composition*, because a model that believes it is composing refuses every key, so a
    // field left this way is a field that never types again.
    let mut script = controlled("ab", Owner::Verbatim);
    script.ime(ImeEvent::Enabled);
    // The cursor an input method reports sits *inside* the candidate text, and here at its start —
    // which is what makes the commit's own caret movement visible. A preedit whose cursor is
    // already at the end leaves nothing for the commit to do, and every assertion about where the
    // caret finished would hold just as well if the commit never moved it.
    script.ime(ImeEvent::Preedit {
        text: "に".into(),
        cursor: Some(0..0),
    });
    assert_eq!(
        script.shown(),
        "にab",
        "the fixture never got as far as composing"
    );
    assert_eq!(
        script.caret(),
        Some(0..0),
        "the fixture never got as far as a caret inside the candidate text"
    );
    script.drain();

    script.window_focus(false);
    assert_eq!(
        script.shown(),
        "にab",
        "the provisional text was taken back out from under the person who typed it"
    );
    assert_eq!(
        script.drain_settled(),
        vec!["にab".to_owned()],
        "a composition that was hanging when the window left settled nothing"
    );
    // And the caret is after what was committed rather than where the composition started. A
    // composition ends by the text it was showing becoming real, and the caret of a commit is at
    // its end; leaving it at the start types the next word in front of the one just finished.
    assert_eq!(
        script.caret(),
        Some(3..3),
        "the caret was left in front of the text the composition committed"
    );

    script.window_focus(true);
    script.type_letter("q");
    assert_eq!(
        script.shown(),
        "にqab",
        "the composition is still open, so every key from here is refused for ever"
    );
    // And what was composed is one undoable change rather than none.
    script.press(letter("z"), Modifiers::CONTROL);
    script.press(letter("z"), Modifiers::CONTROL);
    assert_eq!(script.shown(), "ab");
}

#[test]
fn the_window_losing_the_keyboard_lets_go_of_what_is_being_pressed() {
    // `:active` is written by a press and cleared by the release that matches it, and the release
    // of a press that was interrupted by another window taking the keyboard is delivered to that
    // other window. Nothing later ever corrects it, so the control stays lit for the life of the
    // window. Read as a computed style, because the state bit is only the question's input: what
    // this asks is whether the rule naming the state still wins.
    let mut script = controlled("ab", Owner::Verbatim);
    assert_eq!(script.field_background(), CALM);

    script.pointer(PointerAction::Pressed, 10.0, 10.0);
    assert_eq!(
        script.field_background(),
        PRESSED,
        "the fixture never got as far as pressing the field"
    );

    script.window_focus(false);
    assert_eq!(
        script.field_background(),
        CALM,
        "the field is still held down under a window that is not even in front"
    );

    // And the press really was let go of rather than merely repainted: the release that eventually
    // arrives belongs to nothing, and the field must not light up again on the next frame.
    script.window_focus(true);
    script.pointer(PointerAction::Released, 10.0, 10.0);
    assert_eq!(script.field_background(), CALM);
}

#[test]
fn the_surface_is_told_the_keyboard_is_gone_and_told_where_the_caret_is_when_it_comes_back() {
    // What an input method is running on. It composes into whichever surface last told it text was
    // wanted, and it puts its candidate window wherever that surface last said the caret was, and
    // it is told neither of those things by anything else — the window system says the focus went
    // and stops. A window that only stopped *dispatching* keys is still the one an input method
    // believes it is typing into, and the composition lands in a window that is not in front.
    let mut script = controlled("ab", Owner::Verbatim);
    script.type_letter("z");
    script.press_named(NamedKey::End, KeyCode::End);
    script.harness.settle(8);
    assert_eq!(script.caret(), Some(3..3));

    // The caret's own rectangle: three cells of the fixed face along, one line tall. Asserted as
    // geometry rather than as "something was reported", because the thing that goes wrong here
    // reports the *field's* box — origin at its corner, forty pixels tall — which is a candidate
    // window at the start of a field the person is three characters into.
    let typing = script
        .told_text_input()
        .expect("the surface was never told text was being typed")
        .expect("text input was turned off while the person was typing");
    assert_eq!(typing.caret_origin, Point::new(CssPx(24.0), CssPx(0.0)));
    assert_eq!(
        typing.caret_size,
        zgui_geom::Size::new(CssPx(1.0), CssPx(24.0))
    );

    script.window_focus(false);
    assert_eq!(
        script.told_text_input(),
        Some(None),
        "the input method still believes it is composing into this window"
    );

    script.window_focus(true);
    let back = script
        .told_text_input()
        .expect("told something")
        .expect("the field was returned to and no input method will compose in it again");
    assert_eq!(
        back.caret_origin,
        Point::new(CssPx(24.0), CssPx(0.0)),
        "coming back put the candidate window at the corner of the field rather than at the caret"
    );
    assert_eq!(
        back.caret_size,
        zgui_geom::Size::new(CssPx(1.0), CssPx(24.0))
    );
}
