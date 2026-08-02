//! The same proof, taken every time the suite runs.

/// What a test that cannot run everywhere does instead: it looks for what it needs, says on
/// standard error that it did not find it, and returns — so the refusal is a fact about the machine
/// rather than a property of the source.
#[test]
fn the_stage_really_does_skip_something() {
    let Some(device) = open() else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    run(device);
    assert_non_vacuous(Counter::Beta, fires, silent);
}
