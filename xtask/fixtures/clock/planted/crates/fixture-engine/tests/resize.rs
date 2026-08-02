//! The planted violation: a budget in a target the gate runs unoptimised, where the assertion is
//! gated on the build profile and therefore never runs at all.

#[test]
fn twenty_widths_under_two_milliseconds() {
    let start = Instant::now();
    work();
    let elapsed = start.elapsed();
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_secs_f64() < 0.002, "twenty widths took {elapsed:?}");
    }
}
