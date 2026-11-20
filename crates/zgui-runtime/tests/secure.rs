//! A secure field through the whole loop: keys, pointer, clipboard and the input method.
//!
//! The document is what everything else reads, so each test reads the document and asserts that
//! it holds bullets. The value itself is read only from the events the field announces.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::{
    ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions, PlatformCx, SurfaceEvent,
    TextInputPurpose,
};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, GetUntracked, Set};
use zgui_view::{BuildCx, EventCx, IntoView, NodeRef, View, ViewHost, events};
use zgui_vocab::{
    Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
    PointerEvent, Timestamp,
};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
                   field { display: block; width: 300px; height: 40px }
                   control { display: block; width: 100px; height: 24px }";

/// One cluster's advance in device pixels, from the fixed face at the initial font size.
const ADVANCE: f32 = 8.0;

/// One bullet.
const BULLET: &str = "\u{2022}";

/// A joined emoji: three characters and one grapheme.
const FAMILY: &str = "\u{1f469}\u{200d}\u{1f4bb}";

/// One scripted window holding a secure field.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The field.
    field: NodeRef,
    /// Whether the field is secure.
    secure: RwSignal<bool, zgui_reactive::LocalStorage>,
    /// Every value the field announced.
    values: Rc<RefCell<Vec<String>>>,
}

impl Script {
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    fn press(&mut self, text: &str, modifiers: Modifiers) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent {
                key: Key::Character(text.into()),
                key_without_modifiers: Key::Character(text.into()),
                physical: PhysicalKey::Code(KeyCode::KeyA),
                location: zgui_vocab::KeyLocation::Standard,
                repeat: false,
            },
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
    }

    fn press_named(&mut self, key: NamedKey, code: KeyCode) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    fn point(&mut self, action: PointerAction, x: f32, y: f32) {
        self.deliver(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// The text the document holds under the field, read straight through.
    fn shown(&self) -> String {
        let window = &self.harness.app().windows()[0];
        let document = window.document().borrow();
        let store = document.store();
        let key = zgui_view_dom::id::to_document(self.field.get_untracked().expect("bound"))
            .expect("a document node");
        let index = store.index_of(key).expect("still live");
        let mut text = String::new();
        let mut child = store.core(index).first_child();
        while let Some(node) = child {
            if let Some(held) = zgui_dom::text::text_of(store, node) {
                text.push_str(held);
            }
            child = store.core(node).next_sibling();
        }
        text
    }

    /// What the surface was last told about text input.
    fn purpose(&self) -> Option<TextInputPurpose> {
        self.harness.platform().offscreens()[0]
            .last_text_input()
            .flatten()
            .map(|told| told.purpose)
    }

    /// What the standard clipboard holds.
    fn clipboard(&self) -> Option<String> {
        self.harness
            .platform()
            .clipboard()
            .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
            .ok()
            .and_then(|data| data.as_text().map(str::to_owned))
    }

    /// The last value the field announced.
    fn value(&self) -> Option<String> {
        self.values.borrow().last().cloned()
    }
}

/// A window holding a field with `content` written under it, secure from the start, focused.
fn scripted(content: &'static str) -> Script {
    let field = NodeRef::new();
    let secure = RwSignal::new_local(true);
    let values: Rc<RefCell<Vec<String>>> = Rc::default();
    let heard = Rc::clone(&values);
    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let heard = Rc::clone(&heard);
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::field()
                        .node_ref(field)
                        .secure(move || secure.get())
                        .on(events::INPUT, move |cx: &mut EventCx<'_, _>| {
                            if let Some(payload) = cx.payload().as_value() {
                                heard.borrow_mut().push(payload.value.to_string());
                            }
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
        field,
        secure,
        values,
    };
    script.harness.settle(8);
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script.harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked(),
        script.field.get_untracked(),
        "the fixture never focused the field"
    );
    script
}

#[test]
fn a_secure_field_types_bullets_and_tells_the_input_method_it_takes_a_password() {
    let mut script = scripted("");
    assert_eq!(script.purpose(), Some(TextInputPurpose::Password));
    for letter in ["p", "w", "d"] {
        script.press(letter, Modifiers::NONE);
    }
    assert_eq!(script.shown(), BULLET.repeat(3));
    assert_eq!(script.value().as_deref(), Some("pwd"));
    assert_eq!(
        script.field.selection(),
        Some(3..3),
        "the selection is reported in the value's offsets"
    );
    // Twice: the second one moves nothing in the model, and the record still has to agree.
    script.field.select_all();
    script.harness.settle(8);
    script.field.select_all();
    script.harness.settle(8);
    assert_eq!(
        script.field.selection(),
        Some(0..3),
        "select all reaches the end of the value, not the end of the bullets"
    );
}

#[test]
fn revealing_shows_the_value_and_keeps_the_caret_and_masking_again_hides_it() {
    let mut script = scripted("");
    script.press("a", Modifiers::NONE);
    script.press("b", Modifiers::NONE);

    script.secure.set(false);
    script.harness.settle(8);
    assert_eq!(script.shown(), "ab");
    assert_eq!(script.field.selection(), Some(2..2));
    assert_eq!(script.purpose(), Some(TextInputPurpose::Normal));

    script.secure.set(true);
    script.harness.settle(8);
    assert_eq!(script.shown(), BULLET.repeat(2));
    assert_eq!(script.purpose(), Some(TextInputPurpose::Password));

    // The undo stack survived both changes.
    script.press("z", Modifiers::PRIMARY);
    assert_eq!(script.value().as_deref(), Some(""));
}

#[test]
fn the_text_a_view_wrote_is_masked_before_it_is_shown() {
    let script = scripted("hunter2");
    assert_eq!(script.shown(), BULLET.repeat(7));
    let log = script.harness.platform().offscreens()[0].a11y_log();
    assert!(!log.is_empty(), "no accessibility update was published");
    let mut masked = false;
    for update in &log {
        let dumped = zgui_a11y::dump(update);
        assert!(
            !dumped.contains("hunter"),
            "the value reached the accessibility tree: {dumped}"
        );
        masked |= dumped.contains(&BULLET.repeat(7));
    }
    assert!(
        masked,
        "the field's bullets never reached the tree, so nothing was checked"
    );
}

#[test]
fn copy_and_cut_leave_the_clipboard_and_the_value_alone() {
    let mut script = scripted("hunter2");
    script
        .harness
        .platform()
        .clipboard()
        .write(
            ClipboardKind::Standard,
            ClipboardData::from("before"),
            ClipboardWriteOptions::default(),
        )
        .expect("an in-memory clipboard takes text");
    script.press("a", Modifiers::PRIMARY);
    script.press("c", Modifiers::PRIMARY);
    script.harness.settle(8);
    assert_eq!(script.clipboard().as_deref(), Some("before"));
    script.press("x", Modifiers::PRIMARY);
    script.harness.settle(8);
    assert_eq!(script.clipboard().as_deref(), Some("before"));
    assert_eq!(script.shown(), BULLET.repeat(7), "a cut deleted the secret");
    assert_eq!(script.value(), None, "nothing was announced");
}

#[test]
fn a_click_on_a_bullet_places_the_caret_in_the_value() {
    let value = format!("a{FAMILY}b");
    let content: &'static str = Box::leak(value.clone().into_boxed_str());
    let mut script = scripted(content);
    assert_eq!(script.shown(), BULLET.repeat(3));
    let line = support::first_line_on_device(&script.harness.app().windows()[0]);
    let y = line.origin.y.0 + line.size.height.0 / 2.0;

    script.point(PointerAction::Pressed, line.origin.x.0 + ADVANCE * 2.6, y);
    script.point(PointerAction::Released, line.origin.x.0 + ADVANCE * 2.6, y);
    assert_eq!(
        script.field.selection(),
        Some(value.len()..value.len()),
        "a click after the last bullet is the end of the value"
    );

    script.point(PointerAction::Pressed, line.origin.x.0 + ADVANCE * 1.6, y);
    script.point(PointerAction::Released, line.origin.x.0 + ADVANCE * 1.6, y);
    let after_family = 1 + FAMILY.len();
    assert_eq!(
        script.field.selection(),
        Some(after_family..after_family),
        "a click after the second bullet is after the emoji"
    );
}
