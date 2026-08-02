//! The fixture's budget, in the one target the gate runs in release.

#[test]
fn twenty_widths_stay_under_the_budget() {
    let start = Instant::now();
    work();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(2),
        "twenty widths took {elapsed:?}"
    );
}
