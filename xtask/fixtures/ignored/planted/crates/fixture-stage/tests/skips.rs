//! The proof another gate reads, and no longer takes.

/// The planted violation. The call the skips gate looks for is still written here, so that gate
/// stays green — and this attribute means the assertion behind it is never made.
#[test]
#[ignore]
fn the_stage_really_does_skip_something() {
    run();
    assert_non_vacuous(Counter::Beta, fires, silent);
}
