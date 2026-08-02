//! One test target a gate runs, and the assertions it refuses to run without.

/// A target, and what makes running it mean something.
pub(crate) struct Subject {
    /// The member the target belongs to.
    pub(crate) member: &'static str,
    /// The `--test` name.
    pub(crate) target: &'static str,
    /// Why the gate runs it, in one sentence a failure can quote.
    pub(crate) about: &'static str,
    /// The assertions that must still exist in it.
    pub(crate) required: &'static [&'static str],
    /// Where the list saying so lives, so a failure can point at what to edit.
    pub(crate) listed_in: &'static str,
}

impl Subject {
    /// What to say when this target no longer holds an assertion the gate runs it for.
    pub(crate) fn missing(&self, gate: &str, required: &str, listed: &[String]) -> String {
        format!(
            "`{required}` is gone from `{}`, which the {gate} gate runs because {}. Running the \
             target anyway would leave the gate green over the claim it exists to make. Restore \
             the assertion, or — if it was renamed or moved deliberately — say so in `{}`. What \
             the target holds now: {}",
            self.target,
            self.about,
            self.listed_in,
            if listed.is_empty() {
                "nothing".to_owned()
            } else {
                listed.join(", ")
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Subject;

    /// A subject to build messages from.
    const SUBJECT: Subject = Subject {
        member: "zgui-runtime",
        target: "evict_budget",
        about: "a cache over its soft limit comes back under it",
        required: &["atlas_evicts_when_over_soft_limit"],
        listed_in: "xtask/src/budget/subject.rs",
    };

    #[test]
    fn the_failure_names_the_assertion_the_claim_and_the_file_to_edit() {
        let message = SUBJECT.missing("budget", "atlas_evicts_when_over_soft_limit", &[]);
        assert!(
            message.contains("atlas_evicts_when_over_soft_limit"),
            "{message}"
        );
        assert!(message.contains("comes back under it"), "{message}");
        assert!(message.contains("xtask/src/budget/subject.rs"), "{message}");
    }

    #[test]
    fn an_emptied_target_is_reported_as_holding_nothing_rather_than_as_an_empty_list() {
        assert!(
            SUBJECT
                .missing("budget", "gone", &[])
                .contains("holds now: nothing")
        );
        let other = SUBJECT.missing("budget", "gone", &["something_else".to_owned()]);
        assert!(other.contains("holds now: something_else"), "{other}");
    }
}
