//! A real keymap, real key codes, and the characters that come out.
//!
//! Everything here needs libxkbcommon and the keyboard data behind it, so every test looks for
//! them first. What is asserted is what the layout of this machine has to say whatever it is: `a`
//! is on a key of its own in every Latin layout `xkeyboard-config` ships, and shift over it is
//! upper case in all of them.

mod support;

use zgui_xkb::{Feed, Keycode, Keysym, Layout, Level, RuleNames, Status};

/// `KEY_A`, as `input-event-codes.h` numbers it.
const KEY_A: Keycode = Keycode::from_evdev(30);

/// `KEY_LEFTSHIFT`.
const KEY_LEFTSHIFT: Keycode = Keycode::from_evdev(42);

/// `KEY_LEFTCTRL`.
const KEY_LEFTCTRL: Keycode = Keycode::from_evdev(29);

/// `KEY_CAPSLOCK`.
const KEY_CAPSLOCK: Keycode = Keycode::from_evdev(58);

/// `KEY_RIGHTALT`, which is AltGr on a layout that has one.
const KEY_RIGHTALT: Keycode = Keycode::from_evdev(100);

/// `KEY_Q`, which carries `@` at the third level of a German layout.
const KEY_Q: Keycode = Keycode::from_evdev(16);

/// `KEY_Y`, which is where `z` sits on a German layout and `y` on an American one.
const KEY_Y: Keycode = Keycode::from_evdev(21);

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

/// `XKB_KEY_y`.
const SYM_Y: Keysym = Keysym::from_raw(0x0079);

/// `XKB_KEY_z`.
const SYM_Z: Keysym = Keysym::from_raw(0x007a);

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
    assert_eq!(
        letter.text.as_deref(),
        Some("\u{1}"),
        "control turned the letter into a control character, which is why the shortcut needs \
         the other call"
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

#[test]
fn a_latched_modifier_is_read_before_the_press_that_consumes_it() {
    // This is the test that pins the order inside `press`, and it is the only one that can. A
    // modifier that is *held* changes nothing when the key after it goes down, so reading before
    // and reading after give the same answer for it. A modifier that is *latched* is cleared by
    // the very update that records the next press, so the two answers differ — and the difference
    // is the character the person typed.
    //
    // `lv3:caps_switch_latch` makes caps lock a level-three switch that latches when it is pressed
    // with level three already held. So: hold AltGr, tap caps to latch level three, let AltGr go,
    // and the next key is read at level three. On a German layout that is `@` on the `Q` key.
    let test = "a_latched_modifier_is_read_before_the_press_that_consumes_it";
    let names = RuleNames {
        layout: Some("de".to_owned()),
        options: Some("lv3:caps_switch_latch".to_owned()),
        ..RuleNames::default()
    };
    let Some(keymap) = support::keymap_from(test, &names) else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    state.press(KEY_RIGHTALT);
    state.press(KEY_CAPSLOCK);
    state.release(KEY_CAPSLOCK);
    let released = state.release(KEY_RIGHTALT);
    assert!(
        released.modifiers,
        "letting AltGr go leaves the latch behind"
    );

    let latched = state.press(KEY_Q);

    // Reading after the update answers `q` here, because the update consumed the latch. That is
    // the bug this crate's `press` exists to make unwritable.
    assert_eq!(
        latched.text.as_deref(),
        Some("@"),
        "the latch was read before the press that spent it"
    );

    // And the latch is gone afterwards. The ordering matters for the one key that spends it.
    assert_eq!(
        state.press(KEY_Q).text.as_deref(),
        Some("q"),
        "a latch is spent by one key"
    );
}

#[test]
fn a_second_layout_is_read_at_its_own_level_zero() {
    // `syms_at_level` wraps a layout index past the last one rather than refusing it, so a keymap
    // with one layout answers layout zero for every index and the argument goes unchecked. Two
    // layouts give the argument a meaning: `Y` carries `y` in the first and `z` in the second, and
    // nothing about the state changes between the two readings.
    let test = "a_second_layout_is_read_at_its_own_level_zero";
    let names = RuleNames {
        layout: Some("us,de".to_owned()),
        ..RuleNames::default()
    };
    let Some(keymap) = support::keymap_from(test, &names) else {
        return;
    };

    assert_eq!(
        keymap.unmodified_sym(KEY_Y, Layout::FIRST),
        Some(SYM_Y),
        "the American layout has `y` where the German one has `z`"
    );
    assert_eq!(
        keymap.unmodified_sym(KEY_Y, Layout::from_raw(1)),
        Some(SYM_Z)
    );
}

#[test]
fn a_keyboard_that_was_already_in_use_starts_where_it_was_left() {
    // A keyboard is opened in whatever state somebody left it, and nothing that arrives afterwards
    // says so. Feeding the press of a key that is already down would be wrong for caps lock, which
    // is on with its key up, so the two are set together from what the kernel reports.
    let Some(keymap) =
        support::keymap("a_keyboard_that_was_already_in_use_starts_where_it_was_left")
    else {
        return;
    };
    let mut state = keymap.state().expect("a keymap makes a state");

    // What `EVIOCGKEY` reports: shift is under a finger.
    let changed = state.hold(KEY_LEFTSHIFT);
    // What `EVIOCGLED` reports: caps lock is on with its key up, so it takes both transitions.
    state.hold(KEY_CAPSLOCK);
    state.release(KEY_CAPSLOCK);

    assert!(changed.modifiers);
    assert!(state.modifiers().shift, "shift was down before we looked");
    assert!(state.locked().caps, "and caps lock was on");
    // Shift and caps together are the case that proves both reached the state: caps alone would
    // give `A`, and shift over caps gives it back.
    assert_eq!(state.text(KEY_A).as_deref(), Some("a"));

    state.release(KEY_LEFTSHIFT);
    assert!(!state.modifiers().shift, "and the release is believed");
    assert_eq!(state.text(KEY_A).as_deref(), Some("A"));
}

#[test]
fn a_keymap_that_cannot_compile_says_what_the_library_said() {
    // libxkbcommon reports why a keymap refused through its log and nowhere else — the call itself
    // answers with nothing at all. Without the capture around it, this error would carry only the
    // names that were asked for, and the reason would have gone to standard error.
    let test = "a_keymap_that_cannot_compile_says_what_the_library_said";
    let Some(context) = support::context(test) else {
        return;
    };
    let names = RuleNames {
        rules: Some("zgui-there-is-no-such-rules-file".to_owned()),
        ..RuleNames::default()
    };

    let error = match context.keymap(&names) {
        Err(error) => error,
        Ok(_) => {
            eprintln!(
                "{test}: this machine compiled a keymap from a rules file that does not exist"
            );
            return;
        }
    };

    let message = error.to_string();
    println!("{message}");
    assert!(
        message.contains("zgui-there-is-no-such-rules-file"),
        "the names that were asked for are named: {message}"
    );
    assert!(
        message.len() > "no keymap compiles from rules=zgui-there-is-no-such-rules-file".len(),
        "and libxkbcommon's own reason is carried with them: {message}"
    );
}

#[test]
fn the_diagnostics_reach_a_sink_the_caller_set() {
    // libxkbcommon writes to standard error unless it is told otherwise, and on a bare console
    // that is the screen the framework is drawing on. Nothing is written now, so this is the only
    // way to see what it had to say.
    let test = "the_diagnostics_reach_a_sink_the_caller_set";
    let Some(context) = support::context(test) else {
        return;
    };

    let collected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = std::rc::Rc::clone(&collected);
    context.set_log_sink(Some(Box::new(move |level, text| {
        sink.borrow_mut().push(format!("{level:?}: {text}"));
    })));

    let names = RuleNames {
        rules: Some("zgui-there-is-no-such-rules-file".to_owned()),
        ..RuleNames::default()
    };
    let _ = context.keymap(&names);
    context.set_log_sink(None);

    let said = collected.borrow();
    assert!(
        !said.is_empty(),
        "libxkbcommon had something to say about a rules file that does not exist"
    );
    for line in said.iter() {
        println!("{line}");
    }
}
