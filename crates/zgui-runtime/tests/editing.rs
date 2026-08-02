//! Typing into a focused editable element, through the whole loop.
//!
//! Everything the editing model does is tested against the model in its own crate. What cannot be
//! tested there is whether anything ever *asks* it: a model that is never reached is correct in
//! every one of its own tests and does nothing at all in an application. So every event here is a
//! platform event handed to a real window, the text asserted on is read back out of the document,
//! and the only thing the test knows about the model is that it exists.
//!
//! The three failures this exists to catch are all silent. Editing that is not reached types
//! nothing. Editing that runs *before* the listeners types into a field whose handler refused the
//! key. And a key that arrives during a composition — which the window system keeps delivering —
//! reaching the framework's own behaviour moves the focus or activates a button out from under the
//! provisional text.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View, ViewHost};
use zgui_vocab::{
    ImeEvent, Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   editor { display: block; width: 200px; height: 40px }
                   control { display: block; width: 100px; height: 24px }";

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

/// One scripted window holding an editor.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The editable element.
    editor: NodeRef,
    /// How many clicks reached the editor.
    clicks: Rc<Cell<u32>>,
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

    /// The text the editor's own nodes hold, read straight through.
    ///
    /// Concatenated rather than joined: a node holds its paragraph and the break that ends it, so
    /// the text under the element read in order *is* the text the model holds, and joining would
    /// report a break the document does not have.
    fn text(&self) -> String {
        let window = &self.harness.app().windows()[0];
        let document = window.document().borrow();
        let store = document.store();
        let key = zgui_view_dom::id::to_document(self.editor.get_untracked().expect("bound"))
            .expect("the editor names a document node");
        let index = store.index_of(key).expect("the editor is still live");
        let mut paragraphs = Vec::new();
        let mut child = store.core(index).first_child();
        while let Some(node) = child {
            if let Some(text) = zgui_dom::text::text_of(store, node) {
                paragraphs.push(text.to_owned());
            }
            child = store.core(node).next_sibling();
        }
        paragraphs.concat()
    }

    /// What the framework reports as selected in the editor.
    fn selection(&self) -> Option<core::ops::Range<usize>> {
        self.editor.selection()
    }

    /// What the surface was last told about text input, if it has been told anything.
    fn text_input(&self) -> Option<Option<zgui_platform::TextInput>> {
        self.harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .last_text_input()
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

/// A window holding an editor with `content` in it, and a control after it to tab to.
///
/// `refuse` is what the editor's own key handler does: with it set the handler takes
/// responsibility for every key, which is what an application writing a numeric-only field does.
fn scripted(content: &'static str, refuse: bool) -> Script {
    let editor = NodeRef::new();
    let clicks = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&clicks);

    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let counted = Rc::clone(&counted);
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::editor()
                        .node_ref(editor)
                        .on(
                            zgui_view::events::KEY_DOWN,
                            move |cx: &mut zgui_view::EventCx<'_, _>| {
                                if refuse {
                                    cx.prevent_default();
                                }
                            },
                        )
                        .on(zgui_view::events::CLICK, move |_| {
                            counted.set(counted.get() + 1);
                        })
                        .child(content),
                )
                .child(zgui_elements::control())
                .into_view()
                .build(cx),
        )
    });

    let mut script = Script {
        harness,
        editor,
        clicks,
    };
    script.harness.settle(8);
    // Tab into the window: the editor is the first focusable thing in it.
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script.focused(),
        script.editor.get_untracked(),
        "the fixture never got as far as focusing the editor"
    );
    script
}

#[test]
fn a_key_press_reaching_a_focused_editor_writes_into_the_document() {
    let mut script = scripted("ab", false);
    script.press(letter("z"), Modifiers::NONE);
    assert_eq!(
        script.text(),
        "zab",
        "the key never reached the editing model"
    );
    assert_eq!(
        script.selection(),
        Some(1..1),
        "and the caret it left behind is what the framework reports as selected"
    );

    // A second key reaches the same model rather than a fresh one, which a caret still at the
    // start after two letters would show.
    script.press(letter("y"), Modifiers::NONE);
    assert_eq!(script.text(), "zyab");
    assert_eq!(script.selection(), Some(2..2));
}

#[test]
fn a_handler_that_takes_responsibility_for_the_key_types_nothing() {
    // Editing is a default action, so it must run after every listener and only if none of them
    // refused the event. A framework that typed first would make a numeric-only field impossible.
    let mut script = scripted("ab", true);
    // Focusing the field is what put a caret at its start — a focused field shows an insertion
    // point before anything has been typed into it — so the claim here is that the *key* moved
    // nothing, which is that the caret is still exactly where focus left it.
    assert_eq!(script.selection(), Some(0..0));
    script.press(letter("z"), Modifiers::NONE);
    assert_eq!(script.text(), "ab", "the handler refused the key");
    assert_eq!(
        script.selection(),
        Some(0..0),
        "and the key moved the caret"
    );
}

#[test]
fn the_space_that_is_typed_into_an_editor_does_not_also_activate_it() {
    // Space and enter activate whatever has focus, which is right for a button and wrong for a
    // field: the same press cannot both type a character and click the thing it was typed into.
    let mut script = scripted("ab", false);
    script.press_named(NamedKey::Space, KeyCode::Space);
    assert_eq!(script.text(), " ab", "the space was typed");
    assert_eq!(
        script.clicks.get(),
        0,
        "and it also activated the editor it was typed into"
    );

    script.press_named(NamedKey::Enter, KeyCode::Enter);
    assert_eq!(
        script.text(),
        " \nab",
        "the break split the paragraph after the space"
    );
    assert_eq!(script.clicks.get(), 0);
}

#[test]
fn a_commit_lands_at_the_preedit_and_not_where_a_key_during_it_went() {
    let mut script = scripted("abc", false);
    // The caret starts at the beginning of a freshly attached model; put it at the end the way a
    // person typing would, by walking there.
    for _ in 0..3 {
        script.press_named(NamedKey::ArrowRight, KeyCode::ArrowRight);
    }
    assert_eq!(script.selection(), Some(3..3));

    script.ime(ImeEvent::Enabled);
    script.ime(ImeEvent::Preedit {
        text: "にほん".into(),
        cursor: Some(9..9),
    });
    assert_eq!(script.text(), "abcにほん", "the provisional text is shown");

    // The window system keeps delivering the keys the input method did not consume. None of them
    // may move the caret, type, move the focus, or activate anything.
    for _ in 0..2 {
        script.press_named(NamedKey::ArrowLeft, KeyCode::ArrowLeft);
    }
    script.press(letter("z"), Modifiers::NONE);
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script.text(),
        "abcにほん",
        "a key during a composition acted"
    );
    assert_eq!(
        script.focused(),
        script.editor.get_untracked(),
        "a tab during a composition moved the focus out of the field being typed into"
    );

    script.ime(ImeEvent::Commit("日本".into()));
    assert_eq!(script.text(), "abc日本");
    assert_eq!(script.selection(), Some(9..9));

    // And the keys are released again.
    script.press_named(NamedKey::ArrowLeft, KeyCode::ArrowLeft);
    assert_eq!(script.selection(), Some(6..6));
}

#[test]
fn a_selection_written_from_a_view_is_what_the_next_keystroke_replaces() {
    // The runtime owns the selection and the model owns the caret, and if those are two answers
    // the field types at the wrong place: selecting from a component and then typing is exactly
    // how a "clear and retype" control works.
    let mut script = scripted("abcdef", false);
    script.editor.select_all();
    script.harness.settle(4);
    assert_eq!(script.selection(), Some(0..6));

    script.press(letter("z"), Modifiers::NONE);
    assert_eq!(
        script.text(),
        "z",
        "typing over a selection the view made left the old text behind"
    );

    script.editor.set_selection(0..1);
    script.harness.settle(4);
    script.press_named(NamedKey::Backspace, KeyCode::Backspace);
    assert_eq!(script.text(), "", "backspace removed the selected range");
}

#[test]
fn a_composition_that_ends_without_committing_leaves_the_window_working() {
    // Both Linux backends end a composition that produced nothing with an empty preedit and
    // nothing behind it — no commit, no dismissal, ever. A window that went on treating that as a
    // running composition would refuse every key and every framework behaviour for the rest of its
    // life, and nothing on the screen would say why.
    let mut script = scripted("ab", false);
    script.ime(ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    assert_eq!(script.text(), "にab");
    script.ime(ImeEvent::Preedit {
        text: String::new().into(),
        cursor: None,
    });
    assert_eq!(script.text(), "ab", "the provisional text is gone");

    script.press(letter("z"), Modifiers::NONE);
    assert_eq!(script.text(), "zab", "the window stopped taking keys");
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_ne!(
        script.focused(),
        script.editor.get_untracked(),
        "and stopped moving the focus"
    );
}

#[test]
fn focusing_an_editor_tells_the_surface_text_is_being_typed_and_where() {
    // Until the surface is told, no input method starts a composition at all: a field that never
    // reports this is a field a Japanese keyboard cannot type into, and nothing on the screen says
    // so. What is reported is the caret's own rectangle, so a candidate window opens beside the
    // insertion point: one device pixel wide and one line tall, at the start of a field just
    // focused. The editor's box — 200 by 40 — is what this used to be, and it puts a candidate
    // window at the corner of the field no matter how far along it the person is.
    let script = scripted("ab", false);
    let told = script
        .text_input()
        .expect("the surface was never told anything about text input")
        .expect("focusing an editor has to enable text input");
    assert_eq!(
        told.caret_origin,
        zgui_geom::Point::new(zgui_geom::CssPx(0.0), zgui_geom::CssPx(0.0))
    );
    assert_eq!(
        told.caret_size,
        zgui_geom::Size::new(zgui_geom::CssPx(1.0), zgui_geom::CssPx(24.0))
    );

    // And tabbing on to something that is not editable turns it off again, so an input method is
    // not left composing into a button.
    let mut script = script;
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_ne!(script.focused(), script.editor.get_untracked());
    assert_eq!(
        script.text_input(),
        Some(None),
        "text input was left enabled on an element that takes no text"
    );
}

#[test]
fn a_read_only_field_is_never_typed_into() {
    let mut script = scripted("ab", false);
    {
        let window = &script.harness.app().windows()[0];
        let key = zgui_view_dom::id::to_document(script.editor.get_untracked().expect("bound"))
            .expect("a document node");
        let document = window.document().borrow();
        let index = document.store().index_of(key).expect("live");
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_state(index, zgui_vocab::UiState::READ_ONLY, true);
            })
            .expect("not poisoned");
    }
    script.harness.settle(8);
    script.press(letter("z"), Modifiers::NONE);
    assert_eq!(script.text(), "ab", "a read-only field took the key");
}

#[test]
fn an_empty_field_with_nothing_under_it_can_be_typed_into() {
    // The state every field on every form is in the first time it is shown, and the one no fixture
    // above is in: each of those starts with text in it, so each starts with a text node to write
    // through. A model over an element with none has one empty paragraph and nothing to project it
    // onto, and every keystroke is applied to the buffer and dropped on the way to the document —
    // so the caret advances, the value is right, and the screen stays blank.
    let editor = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::editor().node_ref(editor))
                .child(zgui_elements::control())
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let mut script = Script {
        harness,
        editor,
        clicks: Rc::new(Cell::new(0)),
    };
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(script.focused(), script.editor.get_untracked());

    script.press(letter("h"), Modifiers::NONE);
    script.press(letter("i"), Modifiers::NONE);
    assert_eq!(
        script.text(),
        "hi",
        "the keys reached the model and never reached the document"
    );
    assert_eq!(script.selection(), Some(2..2));

    // And the nodes it wrote through are one per paragraph, not one per keystroke.
    let nodes = {
        let window = &script.harness.app().windows()[0];
        let document = window.document().borrow();
        let store = document.store();
        let key = zgui_view_dom::id::to_document(script.editor.get_untracked().expect("bound"))
            .expect("the editor names a document node");
        let index = store.index_of(key).expect("live");
        let mut count = 0;
        let mut child = store.core(index).first_child();
        while let Some(node) = child {
            count += 1;
            child = store.core(node).next_sibling();
        }
        count
    };
    assert_eq!(nodes, 1, "one paragraph, so one text node");

    // And the letters are on the screen. Text in the document is not text a person can see: the
    // whole of this defect was a field that held the right string, reported the right value and
    // put the caret in the right place while drawing nothing, and every assertion above it is
    // satisfied by exactly that. So the claim is made against the display list — one sprite per
    // typed character, each with a tile that has area to sample.
    let window = &script.harness.app().windows()[0];
    let sprites = &window.scene().primitives.mono_sprites;
    assert_eq!(
        sprites.len(),
        2,
        "two typed characters must be two glyphs in the display list, and the field drew {} — \
         the document is right and the screen is blank",
        sprites.len()
    );
    for sprite in sprites {
        assert!(
            sprite.tile.bounds[2] > 0 && sprite.tile.bounds[3] > 0,
            "a sprite reading an empty rectangle of the atlas draws nothing: {:?}",
            sprite.tile
        );
    }
}

/// How far the moved field is carried, in CSS pixels.
const CARRIED: (f32, f32) = (50.0, 30.0);

/// A field under a transform, so that the space its lines are in is not the device's.
const MOVED_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         editor { display: block; width: 200px; height: 40px }
                         .moved { transform: translate(50px, 30px) }";

#[test]
fn an_input_method_is_told_where_the_caret_is_drawn_and_not_where_it_was_measured() {
    // A caret is planned against the line's own fragment, which keeps its rectangle in the space
    // the paragraph was laid out in. An input method places its candidate window against the
    // screen, so reporting that rectangle opens the window beside where the field would be if
    // nothing above it had moved — a candidate list floating over a different part of the page
    // than the characters being composed, with every glyph and every offset correct.
    let editor = NodeRef::new();
    let mut harness = support::app_with_text(MOVED_CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::editor()
                        .class("moved")
                        .node_ref(editor)
                        .child("ab"),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);

    let told = harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .last_text_input()
        .expect("the surface was never told anything about text input")
        .expect("focusing an editor has to enable text input");
    assert_eq!(
        (told.caret_origin.x.0, told.caret_origin.y.0),
        CARRIED,
        "the field is drawn {CARRIED:?} from where it was laid out, and the candidate window opens \
         against the screen"
    );
    assert_eq!(
        told.caret_size,
        zgui_geom::Size::new(zgui_geom::CssPx(1.0), zgui_geom::CssPx(24.0)),
        "a translation moves the caret without resizing it"
    );
    harness.shut_down();
}
