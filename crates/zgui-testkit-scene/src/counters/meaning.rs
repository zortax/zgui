//! Which counters mean anything under a renderer that draws nothing.

use zgui_profile::{Counter, Counters};

/// The counters a capture renderer cannot move, in declaration order.
///
/// They are incremented by the graphics backend, so under this crate's renderer they read zero
/// because nothing drew — not because nothing was drawn. Every other counter is produced by a
/// stage that does not know what draws its result and means exactly what it says here.
///
/// This list is written out rather than derived so that a *new* renderer-specific counter fails the
/// test beside it and has to be considered, instead of quietly joining a set nothing enumerates.
pub const RENDERER_SPECIFIC: [Counter; 3] = [
    Counter::DrawCalls,
    Counter::DamagePx,
    Counter::BytesUploaded,
];

/// Whether an assertion on `counter` means anything under a capture renderer.
///
/// The question is only about the renderer. A counter of *avoided* work carries a second hazard —
/// it reads zero whether the stage is perfect or absent — but that hazard is the same under every
/// renderer, so it is not this function's to refuse and is settled by
/// [`assert_non_vacuous`](zgui_profile::counter::non_vacuity::assert_non_vacuous) instead.
pub fn is_meaningful(counter: Counter) -> bool {
    counter.group().is_backend_neutral()
}

/// The value a renderer-specific counter reads in a snapshot handed to a test.
///
/// Refusing them *by name* only closes the door a test walks through deliberately. A whole-snapshot
/// read reaches the same fields with no name to refuse, and `draw_calls == 0` there is the exact
/// assertion that must fail loudly rather than pass while measuring nothing. So the fields are not
/// zeroed — a zero is what the wrong assertion is written against — they are filled with a value no
/// equality and no upper bound holds for.
pub const POISON: u64 = u64::MAX;

/// `counters` with every renderer-specific counter replaced by [`POISON`].
///
/// ```
/// use zgui_profile::{Counter, Counters};
/// use zgui_testkit_scene::counters::meaning::{POISON, poisoned};
///
/// let snapshot = poisoned(Counters::from_fn(|_| 0));
/// assert_eq!(snapshot.primitives_emitted, 0);
/// assert_eq!(snapshot.draw_calls, POISON);
/// ```
pub fn poisoned(counters: Counters) -> Counters {
    Counters::from_fn(|counter| {
        if is_meaningful(counter) {
            counters.get(counter)
        } else {
            POISON
        }
    })
}

/// What to say when a test asserts on a counter that cannot mean anything here.
pub fn refusal(counter: Counter) -> String {
    format!(
        "`{}` is a renderer-specific counter and reads zero under a renderer that submits no work, \
         so an assertion on it here would pass without measuring anything. Assert it against a \
         real graphics backend, or assert a backend-neutral counter instead.",
        counter.name()
    )
}

#[cfg(test)]
mod tests {
    use zgui_profile::{Counter, Group};

    use super::{POISON, RENDERER_SPECIFIC, is_meaningful, poisoned, refusal};

    #[test]
    fn the_written_list_is_exactly_the_renderer_specific_group() {
        // A counter added to the group without being added here would be assertable while meaning
        // nothing; one added here without being in the group would be refused while meaning
        // something. Both are failures, so the two sets are compared rather than one derived.
        let from_group: Vec<Counter> = Counter::ALL
            .into_iter()
            .filter(|counter| counter.group() == Group::RendererSpecific)
            .collect();
        assert_eq!(from_group, RENDERER_SPECIFIC.to_vec());
    }

    #[test]
    fn the_counters_a_budget_actually_uses_are_meaningful() {
        for counter in [
            Counter::ElementsRestyled,
            Counter::SelectorMatches,
            Counter::NodesVisited,
            Counter::DirtyWalkSteps,
            Counter::PrimitivesEmitted,
            Counter::VelloPasses,
            Counter::VectorClipLayers,
            Counter::Repaints,
            Counter::TextShaped,
        ] {
            assert!(
                is_meaningful(counter),
                "{} must be assertable",
                counter.name()
            );
        }
        for counter in RENDERER_SPECIFIC {
            assert!(!is_meaningful(counter));
        }
    }

    #[test]
    fn a_whole_snapshot_read_reaches_a_poison_and_never_a_zero() {
        // The hole a by-name refusal cannot close: reading the field off a snapshot names nothing,
        // so `draw_calls == 0` would hold on a renderer that drew nothing and on one that drew.
        let snapshot = poisoned(zgui_profile::Counters::from_fn(|_| 3));
        for counter in RENDERER_SPECIFIC {
            assert_eq!(snapshot.get(counter), POISON, "{}", counter.name());
        }
        assert_eq!(snapshot.primitives_emitted, 3, "the rest are untouched");
    }

    #[test]
    fn the_refusal_names_the_counter_and_says_what_to_do() {
        let message = refusal(Counter::DrawCalls);
        assert!(message.contains("draw_calls"));
        assert!(message.contains("real graphics backend"));
    }
}
