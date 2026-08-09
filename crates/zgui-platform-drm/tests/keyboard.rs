//! A real layout, real key codes, and the readings that come out.
//!
//! Everything here needs a layout source — libxkbcommon with its keyboard data, or a console this
//! process may read — so every test looks for one first and says on standard error when it found
//! neither. `cargo xtask ledger ignored` forbids switching a test off and states that as the
//! alternative, so a refusal is a fact about the machine printed where it happened.
//!
//! What is asserted is what any Latin layout has to say: `a` is on a key of its own, shift over it
//! is upper case, and the key it is a shortcut for is still `a`. Nothing here asserts which layout
//! this machine is set to.

#![cfg(target_os = "linux")]

use zgui_evdev::Key;
use zgui_platform_drm::input::keyboard::layout::{self, Layout};
use zgui_platform_drm::input::seat;
use zgui_vocab::{Modifiers, NamedKey};

/// Returns a layout this machine has, or nothing with the reasons printed.
fn layout(test: &str) -> Option<Box<dyn Layout>> {
    let found = layout::find();
    for refusal in &found.refused {
        println!("{test}: refused: {refusal}");
    }
    match &found.layout {
        Some(layout) => println!("{test}: reading {}", layout.describe()),
        None => eprintln!(
            "{test}: this machine has neither libxkbcommon with its keyboard data nor a console \
             whose keymap this process may read, so nothing was asserted; install \
             `libxkbcommon` and `xkeyboard-config`, or run this on a virtual console"
        ),
    }
    found.layout
}

#[test]
fn a_layout_says_which_source_it_read() {
    // Which source a program got is the first thing anybody wants to know when the wrong letters
    // appear, so it is a value rather than a comment.
    let Some(layout) = layout("a_layout_says_which_source_it_read") else {
        return;
    };

    assert!(
        matches!(
            layout.source(),
            layout::Source::Xkb | layout::Source::Console
        ),
        "a layout in hand came from one of the two sources"
    );
    assert!(
        !layout.describe().is_empty(),
        "the line a person reads says something"
    );
}

#[test]
fn a_press_produces_the_character_the_layout_puts_on_the_key() {
    let Some(mut layout) = layout("a_press_produces_the_character_the_layout_puts_on_the_key")
    else {
        return;
    };

    let press = layout.press(Key::KEY_A);

    // Every Latin layout `xkeyboard-config` ships puts a letter here, and the key code is where
    // the letter `a` is on all but Dvorak. What is asserted is that something was typed at all: a
    // reading with no character is a keymap this backend read through the wrong offset.
    assert!(
        press.key.inserted_text().is_some(),
        "the key marked `A` types something: {:?}",
        press.key
    );
    assert!(press.without_modifiers.inserted_text().is_some());
}

#[test]
fn shift_held_puts_the_next_key_on_its_upper_level_and_leaves_the_shortcut_alone() {
    let Some(mut layout) =
        layout("shift_held_puts_the_next_key_on_its_upper_level_and_leaves_the_shortcut_alone")
    else {
        return;
    };

    let plain = layout.press(Key::KEY_A);
    layout.release(Key::KEY_A);
    layout.press(Key::KEY_LEFTSHIFT);
    let shifted = layout.press(Key::KEY_A);

    assert_eq!(
        layout.modifiers(),
        Modifiers::SHIFT,
        "shift is held and nothing else is"
    );
    assert_ne!(
        shifted.key, plain.key,
        "shift changed what the key types: {:?} and {:?}",
        plain.key, shifted.key
    );
    // The whole reason there are two layout readings. A shortcut written against this key has to
    // find the same entry whether or not shift is down.
    assert_eq!(
        shifted.without_modifiers, plain.without_modifiers,
        "the shortcut reading followed the modifier"
    );

    layout.release(Key::KEY_LEFTSHIFT);
    assert!(
        layout.modifiers().is_empty(),
        "what came up is no longer held"
    );
}

#[test]
fn a_repeat_reads_the_layout_and_records_nothing() {
    // The trap this interface is shaped around. The kernel reports a held key over and over, and a
    // caller that recorded each one would call the layout's update many times and its release
    // once — so shift would stay down for ever, and every later key would come out in the wrong
    // case. Thirty-two repeats is far more than any key press produces before a release.
    let Some(mut layout) = layout("a_repeat_reads_the_layout_and_records_nothing") else {
        return;
    };

    layout.press(Key::KEY_LEFTSHIFT);
    for _ in 0..32 {
        let repeated = layout.reading(Key::KEY_LEFTSHIFT);
        assert!(
            repeated.key.is_modifier() || repeated.key == zgui_vocab::Key::Unidentified,
            "a repeat still reads the key: {:?}",
            repeated.key
        );
    }
    layout.release(Key::KEY_LEFTSHIFT);

    assert!(
        layout.modifiers().is_empty(),
        "one release balances one press, however many repeats came between them"
    );
}

#[test]
fn a_key_that_was_already_down_reaches_the_layout() {
    // A keyboard is opened in whatever state somebody left it, and nothing that arrives afterwards
    // says so: a modifier held before this process was listening is in the kernel's own map of
    // held keys and in no event. `EVIOCGKEY` reads that map, and every key it reports comes through
    // here.
    let Some(mut layout) = layout("a_key_that_was_already_down_reaches_the_layout") else {
        return;
    };

    layout.hold(Key::KEY_LEFTSHIFT);

    assert_eq!(
        layout.modifiers(),
        Modifiers::SHIFT,
        "shift was down before anything was watching"
    );

    layout.release(Key::KEY_LEFTSHIFT);
    assert!(
        layout.modifiers().is_empty(),
        "and the release the kernel sends balances it"
    );
}

#[test]
fn a_key_whose_meaning_is_a_name_arrives_named() {
    // Escape is what `examples/tty.rs` binds to leave, and it is the case both layouts answer
    // differently: libxkbcommon names the keysym, and a console keymap holds an action for it and
    // is named from the position instead.
    let Some(mut layout) = layout("a_key_whose_meaning_is_a_name_arrives_named") else {
        return;
    };

    let pressed = layout.press(Key::KEY_ESC);

    assert_eq!(
        pressed.key,
        zgui_vocab::Key::Named(NamedKey::Escape),
        "escape arrived as {:?}",
        pressed.key
    );
    assert_eq!(
        pressed.key.inserted_text(),
        None,
        "and it types nothing into a field"
    );
}

#[test]
fn the_space_bar_is_the_named_space_key_rather_than_a_character() {
    // The framework activates whatever has focus on `Key::Named(Space)`. A space that arrived as a
    // character would insert correctly and activate nothing, which is a control that works under a
    // pointer and is dead under a keyboard.
    let Some(mut layout) = layout("the_space_bar_is_the_named_space_key_rather_than_a_character")
    else {
        return;
    };

    let pressed = layout.press(Key::KEY_SPACE);

    assert_eq!(pressed.key, zgui_vocab::Key::Named(NamedKey::Space));
    assert_eq!(pressed.key.inserted_text(), Some(" "));
}

#[test]
fn a_key_that_types_nothing_types_nothing() {
    // Enter produces a carriage return and backspace produces a delete character, and a field that
    // inserted either would look right in every test written against key names and would be
    // unusable.
    let Some(mut layout) = layout("a_key_that_types_nothing_types_nothing") else {
        return;
    };

    for key in [Key::KEY_ENTER, Key::KEY_TAB, Key::KEY_BACKSPACE] {
        let pressed = layout.press(key);
        layout.release(key);
        assert_eq!(
            pressed.key.inserted_text(),
            None,
            "{key:?} arrived as {:?}",
            pressed.key
        );
    }
}

#[test]
fn the_narrow_rule_refuses_a_device_udev_calls_a_keyboard() {
    // Against the devices this machine actually has, because the rule exists for devices nobody
    // writes down: a power button, a laptop's hotkey node, a gaming mouse with macro keys. Each is
    // a keyboard under udev's `ID_INPUT_KEY`, and grabbing one takes a function away from the
    // session with no way to get it back while the program runs.
    let test = "the_narrow_rule_refuses_a_device_udev_calls_a_keyboard";
    let found = match zgui_evdev::discover() {
        Ok(found) => found,
        Err(error) => {
            eprintln!("{test}: /dev/input cannot be read on this machine: {error}");
            return;
        }
    };
    if found.opened.is_empty() {
        eprintln!(
            "{test}: no input device on this machine can be opened, so nothing was asserted; add \
             this user to the `input` group to run it"
        );
        return;
    }

    let mut narrowed = 0;
    for device in &found.opened {
        let types_on = seat::types_on(device.capabilities());
        let udev = device.roles().contains(zgui_evdev::Role::Keyboard);
        println!(
            "{}: {:?} udev={udev} typed-on={types_on}",
            device.path().display(),
            device.name()
        );
        // The narrow rule is a subset of the broad one. A device this seat would take and udev
        // would not is a device with a letter and no key at all, which is no device.
        assert!(
            !types_on || udev,
            "{} is taken by a rule broader than udev's",
            device.path().display()
        );
        narrowed += usize::from(udev && !types_on);
    }

    if narrowed == 0 {
        eprintln!(
            "{test}: every readable device here is either a real keyboard or not a keyboard at \
             all, so the narrowing changed no answer on this machine"
        );
    }
}
