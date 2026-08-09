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
        .filter_map(|key| console.entry(*key, Modifiers::NONE).ok().flatten())
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
            matches!((plain, shifted), (Ok(Some(plain)), Ok(Some(shifted))) if plain != shifted)
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
            .expect("key code zero is in every keymap")
            .expect("key code zero is inside a console keymap");
        if absent == Entry::NoSuchMap {
            let sampled = console
                .entry(Key::KEY_A, modifiers)
                .expect("KEY_A is in every keymap")
                .expect("KEY_A is inside a console keymap");
            assert_eq!(
                sampled,
                Entry::Hole,
                "map {index} reports no map at key code zero, so every other key in it is a hole"
            );
        }
    }
}

#[test]
fn a_code_point_entry_reaches_a_caller_only_on_a_unicode_console() {
    let Some(console) = console("a_code_point_entry_reaches_a_caller_only_on_a_unicode_console")
    else {
        return;
    };
    let mode = console.mode().expect("a console reports its mode");

    // The claim this holds up is the kernel's, and it is the one the module is built around:
    // `vt_kdgkbent` replaces every entry above the packed types with a hole unless the console is
    // in `K_UNICODE`. Everything else about it is asserted off hardware; this needs a console,
    // because it is a statement about what the kernel does.
    //
    // It is also what catches a decoding whose boundary is a type too low. On a kernel with a
    // packed type past the ones this crate names, such an entry arrives in *every* mode, so a
    // decoding that read it as a code point would report one here from a `K_XLATE` console.
    let mut points = Vec::new();
    for index in 0..=u8::MAX {
        let modifiers = Modifiers::from_index(index);
        let present = console
            .entry(Key::new(0), modifiers)
            .expect("key code zero is in every keymap")
            .expect("key code zero is inside a console keymap");
        if present == Entry::NoSuchMap {
            continue;
        }
        for code in 0..=u16::from(u8::MAX) {
            let entry = console
                .entry(Key::new(code), modifiers)
                .expect("a code inside the keymap reads")
                .expect("a code inside the keymap has an index");
            if let Entry::Unicode(point) = entry {
                points.push((index, code, point));
            }
        }
    }

    println!("{:?} holds {} code point entries", mode, points.len());
    if mode.keeps_unicode_entries() {
        // A `us` keymap has none of them and a `de` keymap has its euro sign, so the count is a
        // fact about this machine and stays a printed one.
        for (index, code, point) in points.iter().take(8) {
            println!("  map {index} key {code} is U+{point:04X}");
        }
    } else {
        assert!(
            points.is_empty(),
            "the console is in {mode:?}, where the kernel reports a hole for every code point, \
             so any that arrived came from a packed type read as one: {:?}",
            &points[..points.len().min(8)]
        );
    }
}
