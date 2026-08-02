//! What the cadence gate runs.

use crate::gate::Subject;

/// Where this list lives, for a failure that says what to edit.
const HERE: &str = "xtask/src/cadence/subject.rs";

/// Everything the cadence gate covers.
pub(crate) const SUBJECTS: &[Subject] = &[
    Subject {
        member: "zgui-runtime",
        target: "anim_cadence",
        about: "an animation gets exactly one frame per refresh of the output it is on, at sixty, \
                seventy-five and two hundred and forty hertz, and leaves no deadline when it ends",
        required: &[
            "an_animation_gets_one_frame_per_refresh_on_every_output",
            "an_unrelated_wake_does_not_move_the_moment_the_next_tick_is_owed_at",
            "an_animation_that_finishes_leaves_no_deadline_and_draws_nothing_for_ten_seconds",
        ],
        listed_in: HERE,
    },
    Subject {
        member: "zgui-runtime",
        target: "scroll_cadence",
        about: "the overscroll spring moves the content once per refresh on every output, which \
                is the frame count and the picture count and not only the first",
        required: &[
            "an_overscroll_spring_moves_the_content_once_per_refresh_on_every_output",
            "a_motion_that_begins_on_an_idle_window_does_not_spend_the_time_the_window_was_idle",
        ],
        listed_in: HERE,
    },
];

#[cfg(test)]
mod tests {
    use super::SUBJECTS;

    #[test]
    fn both_motions_the_gate_is_named_for_are_covered() {
        let targets: Vec<&str> = SUBJECTS.iter().map(|subject| subject.target).collect();
        assert!(targets.contains(&"anim_cadence"), "animations");
        assert!(targets.contains(&"scroll_cadence"), "the overscroll spring");
    }

    #[test]
    fn every_subject_names_an_assertion_and_says_why_it_is_here() {
        for subject in SUBJECTS {
            assert!(
                !subject.required.is_empty(),
                "{} would be satisfied by an empty target",
                subject.target
            );
            assert!(!subject.about.is_empty(), "{} says nothing", subject.target);
        }
    }

    #[test]
    fn the_gate_names_a_rate_that_is_not_a_whole_number_of_microseconds() {
        // A cadence held by rounding a refresh interval to something convenient is held at sixty
        // and at two hundred and forty and lost at seventy-five, so the list has to say that the
        // awkward rate is among the ones covered — or a target that quietly dropped it would keep
        // this gate green while covering only the easy two.
        assert!(
            SUBJECTS
                .iter()
                .any(|subject| subject.about.contains("seventy-five")),
            "no subject claims the seventy-five hertz output"
        );
    }
}
