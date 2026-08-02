//! The rejection half of the cell discipline and of the store's shareability.
//!
//! The acceptance half is checked everywhere else in this crate, because the record is declared
//! through the gate and the store carries the assertion. This is the half that has to *fail*, and
//! without it both are a claim rather than a check — each of the three shapes below was written the
//! rejected way first, and each compiled cleanly until the check it violates existed.

#[test]
fn a_field_with_a_borrow_counter_is_rejected() {
    trybuild::TestCases::new().compile_fail("tests/ui/refcell_field.rs");
}

#[test]
fn a_column_carrying_a_handler_is_rejected() {
    trybuild::TestCases::new().compile_fail("tests/ui/listeners_with_handlers.rs");
}

#[test]
fn a_column_carrying_a_delivery_channel_is_rejected() {
    trybuild::TestCases::new().compile_fail("tests/ui/observed_with_sink.rs");
}
