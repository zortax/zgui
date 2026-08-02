//! The `view!` grammar, compiled against this vocabulary.
//!
//! The macro's own crate checks the chain it emits against a string, and a string is free to name
//! a builder method that does not exist, pass an argument of the wrong type, or call a function
//! whose name is a reserved word. The check that the two halves fit is here, because this is the
//! crate that can name both.

#[test]
fn every_element_and_every_attribute_form_compiles_and_runs() {
    let suite = trybuild::TestCases::new();
    suite.pass("tests/ui/vocabulary.rs");
}
