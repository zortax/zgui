//! Key events delivered during a composition, and where the commit that follows lands.
//!
//! The platform keeps delivering the keys an input method did not consume: X11 forwards everything
//! `XFilterEvent` did not take, and text-input-v3 behaves the same. So "no key events during a
//! preedit" is a policy this crate keeps, not something the window system does for it, and the
//! failure it prevents is silent — an arrow key moves the caret out from under the provisional
//! text, the commit lands where the caret went, and a Japanese word arrives inside the word before
//! it.

use zgui_edit::Editor;
use zgui_edit::select::Selection;
use zgui_vocab::{ImeEvent, Key, KeyCode, KeyEvent, Modifiers, NamedKey, PhysicalKey};

/// A press of the left arrow.
fn arrow_left() -> KeyEvent {
    KeyEvent::named(NamedKey::ArrowLeft, PhysicalKey::Code(KeyCode::ArrowLeft))
}

/// A press of a letter key.
fn letter(text: &str) -> KeyEvent {
    KeyEvent {
        key: Key::Character(text.into()),
        key_without_modifiers: Key::Character(text.into()),
        physical: PhysicalKey::Code(KeyCode::KeyA),
        location: zgui_vocab::KeyLocation::Standard,
        repeat: false,
    }
}

/// An editor holding `abc` with the caret at the end.
fn editor() -> Editor {
    let mut editor = Editor::new("abc");
    editor.set_selection(Selection::caret(3));
    editor
}

#[test]
fn a_commit_lands_at_the_preedit_and_not_at_a_caret_an_arrow_key_moved() {
    let mut editor = editor();

    editor.ime(&ImeEvent::Enabled);
    editor.ime(&ImeEvent::Preedit {
        text: "にほん".into(),
        cursor: Some(9..9),
    });
    assert_eq!(editor.text(), "abcにほん", "the provisional text is shown");
    let composing = editor
        .composition()
        .expect("a preedit is a composition")
        .range
        .clone();
    assert_eq!(composing, 3..12);

    // Every one of these arrives while the composition is running, exactly as the window system
    // delivers them, and none of them may reach the model.
    for key in [arrow_left(), arrow_left(), letter("z")] {
        let response = editor.key(&key, Modifiers::NONE);
        assert!(
            !response.handled,
            "a key during a composition is left for whatever else is listening"
        );
    }
    assert_eq!(editor.text(), "abcにほん", "and nothing was typed or moved");
    assert_eq!(
        editor.composition().expect("still composing").range,
        composing,
        "the preedit range is still where the commit will go"
    );

    editor.ime(&ImeEvent::Commit("日本".into()));
    assert_eq!(editor.text(), "abc日本");
    assert_eq!(editor.selection(), Selection::caret(9));
    assert!(!editor.is_composing(), "and the keys are released again");
}

#[test]
fn the_same_arrow_key_moves_the_caret_when_nothing_is_being_composed() {
    // The control for the test above. Without it, an editor that ignored *every* key would pass
    // it, and an editor that ignores every key is not an editor.
    let mut editor = editor();
    let response = editor.key(&arrow_left(), Modifiers::NONE);
    assert!(response.handled);
    assert_eq!(editor.selection(), Selection::caret(2));

    let typed = editor.key(&letter("z"), Modifiers::NONE);
    assert!(typed.handled);
    assert_eq!(editor.text(), "abzc");
}

#[test]
fn keys_are_released_again_when_a_composition_is_dismissed() {
    let mut editor = editor();
    editor.ime(&ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    assert!(!editor.key(&arrow_left(), Modifiers::NONE).handled);

    editor.ime(&ImeEvent::Disabled);
    assert_eq!(editor.text(), "abc", "the provisional text was abandoned");
    assert_eq!(editor.selection(), Selection::caret(3));
    assert!(editor.key(&arrow_left(), Modifiers::NONE).handled);
    assert_eq!(editor.selection(), Selection::caret(2));
}

#[test]
fn a_composition_over_a_selection_replaces_it_once_and_not_once_per_preedit() {
    let mut editor = Editor::new("one two three");
    editor.set_selection(Selection::new(4, 7));

    editor.ime(&ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    assert_eq!(editor.text(), "one に three", "the selection was displaced");

    editor.ime(&ImeEvent::Preedit {
        text: "にほ".into(),
        cursor: None,
    });
    assert_eq!(
        editor.text(),
        "one にほ three",
        "the second preedit replaced the first rather than joining it"
    );

    editor.ime(&ImeEvent::Commit("二".into()));
    assert_eq!(editor.text(), "one 二 three");
}

#[test]
fn a_committed_composition_is_one_undo_however_many_preedits_it_took() {
    let mut editor = editor();
    for provisional in ["に", "にほ", "にほん"] {
        editor.ime(&ImeEvent::Preedit {
            text: provisional.into(),
            cursor: None,
        });
    }
    editor.ime(&ImeEvent::Commit("日本".into()));
    assert_eq!(editor.text(), "abc日本");

    editor.apply(zgui_edit::editor::Command::Undo);
    assert_eq!(
        editor.text(),
        "abc",
        "one undo takes back the whole composition, not one preedit of it"
    );
    assert_eq!(editor.selection(), Selection::caret(3));
}

#[test]
fn the_empty_preedit_a_window_system_sends_before_a_commit_still_commits_at_the_preedit() {
    // Both backends clear the provisional text and *then* commit, in that order and with nothing
    // in between: `Ime::Preedit("")` followed by `Ime::Commit(text)`. A model that treated the
    // empty preedit as the end of the composition would insert the committed text at the caret
    // rather than where the provisional text was, which is only visibly wrong when something moved
    // the caret — so the caret is moved here, by a pointer, which a composition never blocks.
    let mut editor = editor();
    editor.ime(&ImeEvent::Preedit {
        text: "にほん".into(),
        cursor: None,
    });
    editor.apply(zgui_edit::editor::Command::Select(Selection::caret(0)));

    editor.ime(&ImeEvent::Preedit {
        text: String::new().into(),
        cursor: None,
    });
    editor.ime(&ImeEvent::Commit("日本".into()));
    assert_eq!(
        editor.text(),
        "abc日本",
        "the commit landed where the caret was moved to, not where the preedit was"
    );
}

#[test]
fn a_composition_abandoned_without_a_dismissal_does_not_wedge_the_field() {
    // The same empty preedit is also how a composition that produced nothing ends: X11 sends it on
    // `End` and Wayland on a `Done` carrying neither a commit nor a new preedit, and neither
    // follows it with `Ime::Disabled`. A model that kept refusing keys while that composition
    // stood would refuse every key for the rest of the field's life, with nothing on the screen
    // saying why.
    let mut editor = editor();
    editor.ime(&ImeEvent::Preedit {
        text: String::new().into(),
        cursor: None,
    });
    editor.ime(&ImeEvent::Preedit {
        text: "に".into(),
        cursor: None,
    });
    editor.ime(&ImeEvent::Preedit {
        text: String::new().into(),
        cursor: None,
    });
    assert_eq!(editor.text(), "abc", "the provisional text is gone");

    let typed = editor.key(&letter("z"), Modifiers::NONE);
    assert!(typed.handled, "the field stopped taking keys");
    assert_eq!(editor.text(), "abcz");
    assert!(!editor.is_composing());
}

#[test]
fn a_commit_with_no_composition_behind_it_is_ordinary_text() {
    // Some input methods commit without ever sending a preedit — a plain key on a Chinese layout,
    // for instance. That text still has to arrive.
    let mut editor = editor();
    editor.ime(&ImeEvent::Commit("字".into()));
    assert_eq!(editor.text(), "abc字");
}
