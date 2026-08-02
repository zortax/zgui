//! Rule 3, made mechanical: what a non-reactive-zone criterion actually asserts.

/// The three re-run counts a zone criterion is made of.
///
/// Nothing about the zone itself can be inspected: the predicate is private and there is no counter,
/// so a criterion phrased as "asserted by a zone check" has nothing to assert. What *is* observable
/// is behaviour, and it takes three readings rather than one — because the first alone holds just as
/// well when the signal was never written, or when effects are not running at all.
///
/// * `untracked_reruns` — an effect reading a second signal untracked inside the zone, after that
///   signal is written. Must be **0**.
/// * `tracked_reruns` — a control effect reading the same signal outside the zone, after the same
///   write. Must be **1**, and it is what proves the write happened and effects run.
/// * `own_reruns` — the first effect re-running on a write to its *own* signal. Must be **1**, and
///   it is what proves the effect is still live rather than disposed.
///
/// ```
/// use zgui_testkit_scene::fixture::zone::ZoneEvidence;
///
/// ZoneEvidence {
///     untracked_reruns: 0,
///     tracked_reruns: 1,
///     own_reruns: 1,
/// }
/// .assert_isolated();
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZoneEvidence {
    /// How often the effect re-ran on a write to the signal it read untracked.
    pub untracked_reruns: u64,
    /// How often the control effect re-ran on the same write.
    pub tracked_reruns: u64,
    /// How often the effect re-ran on a write to its own signal.
    pub own_reruns: u64,
}

impl ZoneEvidence {
    /// Asserts the whole triple.
    ///
    /// # Panics
    ///
    /// Panics when any of the three readings is wrong, naming which one — and, for the control,
    /// what its being wrong means: a control that did not re-run says the write never landed or
    /// effects are not running, in which case the zero beside it is evidence of nothing.
    pub fn assert_isolated(&self) {
        assert_eq!(
            self.tracked_reruns, 1,
            "the tracked control effect re-ran {} times instead of once. Until it does, the \
             untracked count beside it proves nothing: a signal that was never written, or an \
             engine whose effects never run, produces exactly the same zero.",
            self.tracked_reruns
        );
        assert_eq!(
            self.own_reruns, 1,
            "the effect re-ran {} times on its own signal instead of once, so it is not live and \
             its untracked zero is the zero of a disposed effect.",
            self.own_reruns
        );
        assert_eq!(
            self.untracked_reruns, 0,
            "the effect re-ran {} times on a signal it read untracked inside the zone, so the read \
             was tracked after all.",
            self.untracked_reruns
        );
    }
}

#[cfg(test)]
mod tests {
    use super::ZoneEvidence;

    /// The reading a correct zone produces.
    fn isolated() -> ZoneEvidence {
        ZoneEvidence {
            untracked_reruns: 0,
            tracked_reruns: 1,
            own_reruns: 1,
        }
    }

    #[test]
    fn the_correct_triple_passes() {
        isolated().assert_isolated();
    }

    #[test]
    #[should_panic(expected = "read was tracked after all")]
    fn an_untracked_read_that_re_ran_is_caught() {
        ZoneEvidence {
            untracked_reruns: 1,
            ..isolated()
        }
        .assert_isolated();
    }

    #[test]
    #[should_panic(expected = "proves nothing")]
    fn a_control_that_did_not_re_run_invalidates_the_whole_reading() {
        // This is the case the rule exists for. Without the control, an engine in which no effect
        // ever re-runs produces `untracked_reruns == 0` and the assertion passes.
        ZoneEvidence {
            tracked_reruns: 0,
            ..isolated()
        }
        .assert_isolated();
    }

    #[test]
    #[should_panic(expected = "not live")]
    fn a_disposed_effect_is_caught() {
        ZoneEvidence {
            own_reruns: 0,
            ..isolated()
        }
        .assert_isolated();
    }
}
