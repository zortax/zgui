//! Proof that the coordinate-space and unit markers reject what they claim to reject.
//!
//! The whole point of tagging geometry with a space and a unit is that mixing them is a build
//! failure rather than a subtly wrong pixel. That guarantee is only worth something if it is
//! checked, and it cannot be checked by a test that compiles — so each case below is a small
//! program that must *not* compile, together with the error it must produce.

/// Each program in `tests/ui` must fail to compile with the error recorded beside it.
#[test]
fn mixing_spaces_or_units_does_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
