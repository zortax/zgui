//! What the macros refuse, and what they say when they do.
//!
//! A shader is text the compiler cannot see through, so the value of declaring one in a macro is
//! entirely in what the macro refuses. Each fixture here is one refusal, and its recorded output
//! is the message an application actually gets.

#[test]
fn a_declaration_that_cannot_be_drawn_is_refused_where_it_is_written() {
    trybuild::TestCases::new().compile_fail("tests/refused/*.rs");
}
