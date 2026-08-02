//! What the budget gate runs.

use crate::gate::Subject;

/// Where this list lives, for a failure that says what to edit.
const HERE: &str = "xtask/src/budget/subject.rs";

/// Everything the budget gate covers.
pub(crate) const SUBJECTS: &[Subject] = &[
    Subject {
        member: "zgui-runtime",
        target: "evict_budget",
        about: "a cache that has gone over its soft limit comes back under it within a bounded \
                number of frames, and does not thrash doing so",
        required: &["atlas_evicts_when_over_soft_limit"],
        listed_in: HERE,
    },
    Subject {
        member: "zgui-runtime",
        target: "evict_replay",
        about: "no cached range is replayed naming a raster the cache behind it no longer holds",
        required: &["no_replayed_range_names_an_evicted_tile"],
        listed_in: HERE,
    },
    Subject {
        member: "zgui-runtime",
        target: "evict_pinned",
        about: "eviction at its most aggressive still cannot take a raster something is holding",
        required: &["pinned_resources_survive_aggressive_eviction"],
        listed_in: HERE,
    },
    Subject {
        member: "zgui-runtime",
        target: "budget_registry",
        about: "every registered cache reports itself, comes back under a level it is over, and \
                can be emptied",
        required: &[
            "every_registered_cache_is_empty_after_forget",
            "the_report_names_every_registered_cache_exactly_once",
            "a_cache_comes_back_under_a_level_it_is_over_and_is_untouched_under_one_it_is_not",
        ],
        listed_in: HERE,
    },
];

#[cfg(test)]
mod tests {
    use super::SUBJECTS;

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
    fn the_two_halves_the_gate_is_named_for_are_both_covered() {
        // The gate is one sentence with an "and" in it, and a list that quietly lost half of it
        // would still run, still pass, and still be called `budget`.
        let names: Vec<&str> = SUBJECTS
            .iter()
            .flat_map(|subject| subject.required)
            .copied()
            .collect();
        assert!(names.contains(&"atlas_evicts_when_over_soft_limit"));
        assert!(names.contains(&"no_replayed_range_names_an_evicted_tile"));
    }
}
