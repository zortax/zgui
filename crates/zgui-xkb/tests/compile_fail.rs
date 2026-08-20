//! Proof that a keyboard handle cannot leave the thread that made it.
//!
//! libxkbcommon counts references without a lock, so two threads dropping states over one keymap
//! would race that count into a double free. Nothing in this crate promises otherwise, and today
//! nothing has to: every handle holds a [`std::ptr::NonNull`], which is neither `Send` nor `Sync`,
//! so the guarantee is the type system's.
//!
//! The guarantee rests on a field nobody reads. A refactor that wrapped the pointer in a type of
//! our own would take it away with every test still green, so each program below must fail to
//! compile, with the error recorded beside it.

/// Each program in `tests/ui` must fail to compile with the error recorded beside it.
#[test]
fn a_keyboard_handle_does_not_leave_its_thread() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
