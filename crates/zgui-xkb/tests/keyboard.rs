//! A real keymap, real key codes, and the characters that come out.
//!
//! Everything here needs libxkbcommon and the keyboard data behind it, so every test looks for
//! them first. What is asserted is what the layout of this machine has to say whatever it is: `a`
//! is on a key of its own in every Latin layout `xkeyboard-config` ships, and shift over it is
//! upper case in all of them.

mod support;

use zgui_xkb::{Feed, Keycode, Keysym, Level, Status};

/// `KEY_A`, as `input-event-codes.h` numbers it.
const KEY_A: Keycode = Keycode::from_evdev(30);

/// `KEY_LEFTSHIFT`.
const KEY_LEFTSHIFT: Keycode = Keycode::from_evdev(42);

/// `KEY_LEFTCTRL`.
const KEY_LEFTCTRL: Keycode = Keycode::from_evdev(29);

/// `KEY_CAPSLOCK`.
const KEY_CAPSLOCK: Keycode = Keycode::from_evdev(58);

/// `XKB_KEY_a`.
const SYM_A_LOWER: Keysym = Keysym::from_raw(0x0061);

/// `XKB_KEY_A`.
const SYM_A_UPPER: Keysym = Keysym::from_raw(0x0041);

/// `XKB_KEY_e`.
const SYM_E: Keysym = Keysym::from_raw(0x0065);

/// `XKB_KEY_ae`, which is `æ`.
const SYM_AE: Keysym = Keysym::from_raw(0x00e6);

/// `XKB_KEY_Multi_key`, the compose key.
const SYM_COMPOSE: Keysym = Keysym::from_raw(0xff20);

#[test]
fn a_keymap_compiles_and_a_kernel_key_code_produces_a_character() {
    let Some(keymap) =
        support::keymap("a_keymap_compiles_and_a_kernel_key_code_produces_a_character")
    else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    let press = state.press(KEY_A);

    // This asserts the offset. Handing 30 to libxkbcommon in place of 38 reaches whatever key sits
    // eight positions earlier, which answers a character rather than an error.
    assert_eq!(press.sym, SYM_A_LOWER, "the key marked `a` carries `a`");
    assert_eq!(press.text.as_deref(), Some("a"));
    assert!(
        press.changed.is_empty(),
        "a letter changes no part of the state"
    );
}

#[test]
fn shift_held_puts_the_next_key_on_its_upper_level() {
    let Some(keymap) = support::keymap("shift_held_puts_the_next_key_on_its_upper_level") else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    let shift = state.press(KEY_LEFTSHIFT);
    let letter = state.press(KEY_A);

    // This pins the order inside `press`: the keysym and the text are read before the state is
    // told about the press. Reversing the two inside a modifier's own press makes exactly one key
    // per modifier change come out in the wrong case.
    assert_eq!(shift.text, None, "a modifier produces no text");
    assert!(shift.changed.modifiers, "shift changed the modifier set");
    assert_eq!(letter.sym, SYM_A_UPPER);
    assert_eq!(letter.text.as_deref(), Some("A"));

    state.release(KEY_LEFTSHIFT);
    assert_eq!(
        state.text(KEY_A).as_deref(),
        Some("a"),
        "the key is lower case again once shift is up"
    );
}

#[test]
fn the_key_that_is_printed_is_read_at_level_zero_while_a_modifier_is_held() {
    let Some(keymap) =
        support::keymap("the_key_that_is_printed_is_read_at_level_zero_while_a_modifier_is_held")
    else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    state.press(KEY_LEFTSHIFT);
    state.press(KEY_LEFTCTRL);
    let letter = state.press(KEY_A);

    // A shortcut is written against the letter printed on the key, so `Ctrl+Shift+A` has to find
    // `a` while the state's own answer is neither `a` nor a letter at all. That is a different
    // call: the state reads the level the modifiers put the key on, and this reads level zero.
    let printed = keymap.unmodified_sym(KEY_A, state.layout(KEY_A));
    assert_eq!(printed, Some(SYM_A_LOWER));
    assert_ne!(
        letter.text.as_deref(),
        Some("a"),
        "control took the letter away, which is why the shortcut needs the other call"
    );
    assert_eq!(
        keymap.syms_at_level(KEY_A, state.layout(KEY_A), Level::UNMODIFIED),
        [SYM_A_LOWER],
        "one key, one symbol at the level it is printed with"
    );
}

#[test]
fn modifiers_are_reported_while_they_are_held_and_not_after() {
    let Some(keymap) = support::keymap("modifiers_are_reported_while_they_are_held_and_not_after")
    else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    assert!(
        state.modifiers().is_empty(),
        "a state starts with none held"
    );

    state.press(KEY_LEFTSHIFT);
    assert!(state.modifiers().shift);
    state.press(KEY_LEFTCTRL);
    let both = state.modifiers();
    assert!(both.shift && both.control, "two modifiers are held at once");

    let changed = state.release(KEY_LEFTSHIFT);
    assert!(changed.modifiers);
    assert!(!state.modifiers().shift, "what came up is no longer held");
    assert!(state.modifiers().control, "and what is still down still is");

    state.release(KEY_LEFTCTRL);
    assert!(state.modifiers().is_empty());
}

#[test]
fn caps_lock_stays_on_after_the_key_comes_up() {
    let Some(keymap) = support::keymap("caps_lock_stays_on_after_the_key_comes_up") else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    state.press(KEY_CAPSLOCK);
    state.release(KEY_CAPSLOCK);

    // A locking modifier is the case a state that only tracked held keys gets wrong: the key is up
    // and the modifier is on, and it stays on until the key is pressed again.
    assert!(state.modifiers().caps, "caps is on with the key up");
    assert!(state.locked().caps, "and it is on because it is locked");
    assert_eq!(state.text(KEY_A).as_deref(), Some("A"));

    state.press(KEY_CAPSLOCK);
    state.release(KEY_CAPSLOCK);
    assert!(!state.modifiers().caps, "pressing it again turns it off");
    assert!(!state.locked().caps);
    assert_eq!(state.text(KEY_A).as_deref(), Some("a"));
}

#[test]
fn a_letter_repeats_and_a_modifier_does_not() {
    let Some(keymap) = support::keymap("a_letter_repeats_and_a_modifier_does_not") else {
        return;
    };

    // Which keys repeat is the keymap's decision rather than a rule a caller can write. Holding
    // shift would otherwise fill a document with nothing at all.
    assert!(keymap.key_repeats(KEY_A), "a held letter repeats");
    assert!(
        !keymap.key_repeats(KEY_LEFTSHIFT),
        "a held modifier does not"
    );
    assert!(!keymap.key_repeats(KEY_CAPSLOCK));
}

#[test]
fn a_compose_sequence_produces_its_character() {
    let test = "a_compose_sequence_produces_its_character";
    let Some(context) = support::context(test) else {
        return;
    };
    let table = match context.compose_table("en_US.UTF-8") {
        Ok(table) => table,
        Err(error) => {
            eprintln!(
                "{test}: {error}, so nothing was asserted; install the X11 locale data to run it"
            );
            return;
        }
    };
    let mut compose = table.state().expect("a table makes a state");

    assert_eq!(compose.feed(SYM_COMPOSE), Feed::Accepted);
    assert_eq!(compose.status(), Status::Composing, "a sequence has begun");
    assert_eq!(compose.feed(SYM_A_LOWER), Feed::Accepted);
    assert_eq!(compose.status(), Status::Composing, "and has not finished");
    assert_eq!(compose.feed(SYM_E), Feed::Accepted);

    // Compose, `a`, `e` is `æ` in the default table. While the status was `Composing` the two keys
    // produced nothing to show, and this one replaces what `e` would have produced.
    assert_eq!(compose.status(), Status::Composed);
    assert_eq!(compose.text().as_deref(), Some("æ"));
    assert_eq!(compose.sym(), Some(SYM_AE));

    compose.reset();
    assert_eq!(
        compose.status(),
        Status::Nothing,
        "a reset sequence is gone"
    );
    assert_eq!(compose.text(), None);
}

#[test]
fn a_sequence_that_leads_nowhere_is_cancelled() {
    let test = "a_sequence_that_leads_nowhere_is_cancelled";
    let Some(context) = support::context(test) else {
        return;
    };
    let Ok(table) = context.compose_table("en_US.UTF-8") else {
        eprintln!("{test}: this machine has no compose table, so nothing was asserted");
        return;
    };
    let mut compose = table.state().expect("a table makes a state");

    compose.feed(SYM_COMPOSE);
    compose.feed(SYM_COMPOSE);

    // Two compose keys continue nothing. The sequence is thrown away, and a caller has to show the
    // key rather than swallow it.
    assert_eq!(compose.status(), Status::Cancelled);
    assert_eq!(compose.text(), None);
}

#[test]
fn a_keysym_is_named_the_way_the_keyboard_data_names_it() {
    let Some(context) = support::context("a_keysym_is_named_the_way_the_keyboard_data_names_it")
    else {
        return;
    };

    // The name is what a shortcut table and a log line are keyed by, and it is read through the
    // same two-call buffer as the text.
    assert_eq!(context.keysym_name(SYM_A_LOWER).as_deref(), Some("a"));
    assert_eq!(context.keysym_name(SYM_A_UPPER).as_deref(), Some("A"));
    assert_eq!(
        context.keysym_name(SYM_COMPOSE).as_deref(),
        Some("Multi_key")
    );
    assert_eq!(
        context.keysym_name(Keysym::NONE).as_deref(),
        Some("NoSymbol"),
        "the symbol that is no symbol is named too"
    );

    // A number past the range keysyms are drawn from is the one case with no name at all, and it
    // is the case the buffer pattern would read as a length.
    assert_eq!(context.keysym_name(Keysym::from_raw(u32::MAX)), None);
}
