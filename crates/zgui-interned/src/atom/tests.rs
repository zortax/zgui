//! Tests for [`Atom`](super::Atom) and the pool behind it.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::thread;

use proptest::prelude::*;

use super::Atom;

/// The hash a map would key an atom by.
fn hash(atom: Atom) -> u64 {
    let mut hasher = DefaultHasher::new();
    atom.hash(&mut hasher);
    hasher.finish()
}

proptest! {
    /// Interning the same text twice yields the same handle, whatever the text is.
    #[test]
    fn equal_text_interns_to_one_identity(text in ".{0,32}") {
        let first = Atom::new(&text);
        let second = Atom::new(&text);
        prop_assert!(first.is(second));
        prop_assert_eq!(first, second);
        prop_assert_eq!(hash(first), hash(second));
    }

    /// The text survives the round trip byte for byte.
    #[test]
    fn the_text_round_trips(text in ".{0,32}") {
        prop_assert_eq!(Atom::new(&text).as_str(), text.as_str());
    }

    /// Different text never collides into one identity.
    #[test]
    fn different_text_stays_different(left in ".{0,16}", right in ".{0,16}") {
        prop_assume!(left != right);
        prop_assert_ne!(Atom::new(&left), Atom::new(&right));
    }

    /// Ordering follows the text, so a sorted list of names reads the way a reader expects.
    #[test]
    fn ordering_follows_the_text(left in "[a-z]{0,8}", right in "[a-z]{0,8}") {
        prop_assert_eq!(
            Atom::new(&left).cmp(&Atom::new(&right)),
            left.cmp(&right)
        );
    }
}

#[test]
fn an_atom_is_one_pointer_wide_and_leaves_room_for_a_niche() {
    assert_eq!(size_of::<Atom>(), size_of::<usize>());
    assert_eq!(size_of::<Option<Atom>>(), size_of::<usize>());
}

#[test]
fn the_default_atom_is_the_empty_string() {
    assert_eq!(Atom::default().as_str(), "");
    assert!(Atom::default().is_empty());
    assert_eq!(Atom::default().len(), 0);
}

#[test]
fn interning_is_case_sensitive() {
    assert_ne!(Atom::new("DIV"), Atom::new("div"));
}

#[test]
fn display_and_debug_show_the_text() {
    let atom = Atom::new("flex-basis");
    assert_eq!(atom.to_string(), "flex-basis");
    assert_eq!(format!("{atom:?}"), "\"flex-basis\"");
}

#[test]
fn conversions_reach_the_same_atom_from_either_kind_of_string() {
    let borrowed: Atom = "grid-area".into();
    let owned: Atom = String::from("grid-area").into();
    assert!(borrowed.is(owned));
    assert_eq!(borrowed.as_ref(), "grid-area");
}

#[test]
fn concurrent_interning_of_the_same_text_still_yields_one_identity() {
    let workers: Vec<_> = (0..8)
        .map(|_| thread::spawn(|| Atom::new("simultaneously-interned")))
        .collect();
    let atoms: Vec<Atom> = workers
        .into_iter()
        .map(|worker| worker.join().expect("an interning thread panicked"))
        .collect();
    assert!(atoms.windows(2).all(|pair| pair[0].is(pair[1])));
}
