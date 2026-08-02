//! The occupancy counter that tells successive occupants of one slot apart.

use core::num::NonZeroU16;

/// How many times a slot has been occupied, counting from one.
///
/// The counter is what makes a handle safe to keep: a handle carrying an older counter never
/// resolves to the value that took its slot over. Zero is deliberately not a counter — it is the
/// "never issued" state — which has two consequences worth relying on. [`Option`] of a counter is
/// two bytes, so a table of counters costs no more than a table of raw numbers; and a handle
/// built from a live counter can never be all-zero bits, which is what lets a handle be
/// pointer-sized and still nullable for free.
///
/// The counter is bounded. A slot that has been through [`Generation::LAST`] occupants cannot be
/// handed out again without risking a stale handle resolving to a new value, so it is retired
/// instead: [`Generation::next`] returns [`None`] and the slot is dropped from circulation.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(transparent)]
pub struct Generation(NonZeroU16);

impl Generation {
    /// The counter a slot carries the first time it is occupied.
    pub const FIRST: Self = match Self::new(1) {
        Some(generation) => generation,
        None => panic!("1 is a valid counter"),
    };

    /// The last counter a slot can carry. Its next occupant would need a counter that does not
    /// exist, so the slot is retired instead.
    pub const LAST: Self = match Self::new(u16::MAX) {
        Some(generation) => generation,
        None => panic!("u16::MAX is a valid counter"),
    };

    /// Wraps a raw counter, rejecting the zero that means "never issued".
    pub const fn new(value: u16) -> Option<Self> {
        match NonZeroU16::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// The raw counter, which is never zero.
    pub const fn get(self) -> u16 {
        self.0.get()
    }

    /// The counter the slot's next occupant would carry, or [`None`] if the slot has run out of
    /// counters and must be retired.
    ///
    /// ```
    /// use zgui_arena::Generation;
    ///
    /// assert_eq!(Generation::FIRST.next(), Generation::new(2));
    /// assert_eq!(Generation::LAST.next(), None);
    /// ```
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::Generation;

    #[test]
    fn zero_is_not_a_counter() {
        assert_eq!(Generation::new(0), None);
    }

    #[test]
    fn the_first_counter_is_one() {
        assert_eq!(Generation::FIRST.get(), 1);
    }

    #[test]
    fn an_option_of_a_counter_costs_nothing() {
        assert_eq!(
            size_of::<Option<Generation>>(),
            size_of::<Generation>(),
            "the zero that means `never issued` is the niche `Option` uses"
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config::config())]

        /// Successive counters are distinct and increasing, right up to retirement.
        #[test]
        fn successors_are_strictly_increasing(raw in 1_u16..=u16::MAX) {
            let generation = Generation::new(raw).expect("non-zero");
            match generation.next() {
                Some(next) => prop_assert!(next > generation),
                None => prop_assert_eq!(generation, Generation::LAST),
            }
        }
    }
}
