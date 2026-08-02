//! The guarantee the arena exists for, exercised the way a consumer will use it.
//!
//! A consumer parcels references into the arena out to workers and keeps building the structure
//! those workers are walking. The borrow checker cannot see that the references stay valid — that
//! is precisely what the arena promises and what nothing in the type system can express — so a
//! consumer reaches for a raw pointer, and so does this test. Run under an aliasing-model checker
//! it is the proof that the promise holds.

// This file deliberately does what a consumer of the arena has to do, so that a checker sees it.
#![allow(unsafe_code)]

use zgui_arena::{BLOCK_LEN, ChunkArena, DomainId, Key};

/// Big enough to span many blocks, so growth happens repeatedly while references are held.
const COUNT: u32 = 10_000;

/// Extends a borrow of a value to the arena's own lifetime.
///
/// # Safety
///
/// The value must not be dropped for as long as the returned reference is used, which for an
/// arena means the slot must not be recycled or taken from.
unsafe fn hold<'a, T>(value: &T) -> &'a T {
    // SAFETY: the arena never moves a value, so the address stays valid until the value is
    // dropped, which the caller promises not to do while the reference is in use.
    unsafe { &*(value as *const T) }
}

#[test]
fn references_survive_growth_removal_and_everything_short_of_a_recycle() {
    assert!(
        COUNT as usize > BLOCK_LEN * 4,
        "the arena must have to grow"
    );

    let mut arena: ChunkArena<u64> = ChunkArena::new(DomainId::FIRST);
    let mut keys: Vec<Key<u64>> = Vec::new();
    let mut held: Vec<&u64> = Vec::new();

    for value in 0..u64::from(COUNT) {
        let key = arena.insert(value);
        keys.push(key);
        // SAFETY: nothing below drops a value until the recycle at the very end, after which no
        // held reference is read.
        held.push(unsafe { hold(arena.get(key).expect("just inserted")) });
    }

    // Every reference taken before the arena grew is still the value it named.
    for (value, reference) in held.iter().enumerate() {
        assert_eq!(**reference, value as u64);
    }

    // Growing further does not disturb them.
    for value in 0..u64::from(COUNT) {
        arena.insert(value + u64::from(COUNT));
    }
    for (value, reference) in held.iter().enumerate() {
        assert_eq!(**reference, value as u64);
    }

    // Half the values are removed. Removal defers the drop, so every reference — including the
    // ones into removed slots — is still readable, and so is every key.
    for key in keys.iter().step_by(2) {
        assert!(arena.remove(*key));
    }
    for (value, reference) in held.iter().enumerate() {
        assert_eq!(**reference, value as u64);
    }
    for (value, key) in keys.iter().enumerate() {
        assert_eq!(arena.get(*key), Some(&(value as u64)));
    }

    // The frame ends. The references must not be read past this point, and the keys into the
    // removed half stop resolving.
    drop(held);
    arena.recycle();
    for (value, key) in keys.iter().enumerate() {
        if value % 2 == 0 {
            assert_eq!(arena.get(*key), None);
        } else {
            assert_eq!(arena.get(*key), Some(&(value as u64)));
        }
    }
}

#[test]
fn a_reference_taken_before_a_reuse_is_not_the_reused_slot() {
    let mut arena: ChunkArena<u64> = ChunkArena::new(DomainId::FIRST);
    let first = arena.insert(1);
    // SAFETY: the value is dropped by the recycle below, after the last read of this reference.
    let reference = unsafe { hold(arena.get(first).expect("just inserted")) };
    assert_eq!(*reference, 1);

    arena.remove(first);
    assert_eq!(*reference, 1, "a removed value is still in place");
    arena.recycle();

    let second = arena.insert(2);
    assert_eq!(second.index(), first.index(), "the slot came back round");
    assert_eq!(arena.get(first), None, "the old key does not follow it");
    assert_eq!(arena.get(second), Some(&2));
}
