//! Proof that a seat cannot leave the thread that opened it.

/// Each program in `tests/ui` must fail to compile with the error recorded beside it.
#[test]
fn a_seat_does_not_leave_its_thread() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
