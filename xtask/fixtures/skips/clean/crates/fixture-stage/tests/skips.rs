//! What the stage skips, and where it must not skip anything.

#[test]
fn the_stage_reuses_an_answer_when_it_has_one_and_never_on_the_first_call() {
    assert_non_vacuous(
        Counter::Beta,
        Scenario::new("a second call, which has an answer to reuse", second_call),
        Scenario::new("the first call, which has none", first_call),
    );
}
