//! What the stage costs.

#[test]
fn the_stage_does_not_run_away_with_itself() {
    run();
    assert!(counter::get(Counter::Beta) <= 4, "too much was skipped");
}
