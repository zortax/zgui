//! The keymap a real console holds.
//!
//! Every test here needs a console, so every one looks for one first and says on standard error
//! when there is none. `cargo xtask ledger ignored` forbids switching a test off and states this
//! as the alternative, so the refusal is a fact about the machine, printed where it happened.
//!
//! What is asserted is what any console keymap has to say. A key on the letter row produces a
//! letter, whichever letter a `us`, `de`, `fr` or Dvorak keymap puts there, and shift changes at
//! least one key. Which keymap this machine loaded is printed for a person to read.
//!
//! The probe lives here rather than in `support`, because `tests/device.rs` includes that module
//! and a helper it never calls would be dead code in its binary.

#![cfg(target_os = "linux")]

use zgui_evdev::{Console, Entry, Key, Modifiers};

/// The keys of the top letter row, which every Latin keymap fills with letters.
const LETTER_ROW: [Key; 10] = [
    Key::KEY_Q,
    Key::KEY_W,
    Key::KEY_E,
    Key::KEY_R,
    Key::KEY_T,
    Key::KEY_Y,
    Key::KEY_U,
    Key::KEY_I,
    Key::KEY_O,
    Key::KEY_P,
];

/// Returns a console on this machine, or nothing with every refusal printed.
///
/// The three paths fail in three different ways and each wants something different done about it,
/// so all three reasons are printed. `/dev/tty` under a test harness with its output redirected
/// answers `ENXIO`, because the process has no controlling terminal; under a terminal emulator it
/// opens and answers `ENOTTY`, because a pseudo-terminal is no console. `/dev/tty0` and
/// `/dev/console` belong to root.
fn console(test: &str) -> Option<Console> {
    let found = Console::find();
    for refused in &found.refused {
        println!("refused {}: {}", refused.path.display(), refused.reason);
    }
    if found.console.is_none() {
        eprintln!(
            "{test}: no console on this machine answered, so nothing was asserted; run this from \
             a virtual console, or as a user who may open /dev/tty0"
        );
    }
    found.console
}

#[test]
fn a_key_on_the_letter_row_produces_a_letter() {
    let Some(console) = console("a_key_on_the_letter_row_produces_a_letter") else {
        return;
    };
    println!(
        "{} is in {:?}",
        console.path().display(),
        console.mode().expect("a console reports its mode")
    );

    let letters: Vec<char> = LETTER_ROW
        .iter()
        .filter_map(|key| console.entry(*key, Modifiers::NONE).ok())
        .filter_map(Entry::character)
        .filter(|character| character.is_alphabetic())
        .collect();

    println!("the unshifted letter row is {letters:?}");
    assert!(
        !letters.is_empty(),
        "a console keymap that answers no letter for any of the ten keys a person types letters \
         on is one nothing could type with"
    );
}

#[test]
fn holding_shift_changes_at_least_one_key() {
    let Some(console) = console("holding_shift_changes_at_least_one_key") else {
        return;
    };

    // Every code a console keymap has an entry for. Key code zero is left out: it is
    // `KEY_RESERVED`, and it is the one code the kernel answers `K_NOSUCHMAP` for.
    let changed = (1..=u16::from(u8::MAX))
        .map(Key::new)
        .filter(|key| {
            let plain = console.entry(*key, Modifiers::NONE);
            let shifted = console.entry(*key, Modifiers::SHIFT);
            matches!((plain, shifted), (Ok(plain), Ok(shifted)) if plain != shifted)
        })
        .count();

    println!("{changed} keys read differently with shift held");
    assert!(
        changed > 0,
        "a keymap whose shifted map matched its unshifted one would type one case only"
    );
}

#[test]
fn a_map_the_keymap_never_loaded_says_so_rather_than_reading_as_holes() {
    let Some(console) =
        console("a_map_the_keymap_never_loaded_says_so_rather_than_reading_as_holes")
    else {
        return;
    };

    // Key code zero is the one code that tells an absent map from a map of unbound keys. Which
    // combinations a keymap defines is its own business, so what is asserted is that the two
    // answers agree with each other: a map that reports `NoSuchMap` at code zero holds a hole
    // everywhere, because the kernel has nothing to read.
    for index in 0..=u8::MAX {
        let modifiers = Modifiers::from_index(index);
        let absent = console
            .entry(Key::new(0), modifiers)
            .expect("key code zero is in every keymap");
        if absent == Entry::NoSuchMap {
            let sampled = console
                .entry(Key::KEY_A, modifiers)
                .expect("KEY_A is in every keymap");
            assert_eq!(
                sampled,
                Entry::Hole,
                "map {index} reports no map at key code zero, so every other key in it is a hole"
            );
        }
    }
}
