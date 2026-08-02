//! An ordinary test target: it may read the clock, so long as it does not assert on what it read.

#[test]
fn throughput_is_recorded_rather_than_asserted() {
    let start = Instant::now();
    let styled = work();
    eprintln!("styled {styled} in {:?}", start.elapsed());
    assert_eq!(styled, 4000, "the run styled the whole document");
}
