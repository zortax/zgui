//! The stored form of the record: four slots and a tag saying how to read them.

use crate::id::node_key::OptIndex;
use crate::plain_data;

/// How many children the record names exactly before it degrades to a span.
pub const EXACT: usize = 4;

/// The stored form: four slots, and a tag saying how to read them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) struct Repr {
    /// The marked children, or — when `len` is [`Repr::SPAN`] — the first and last of them on the
    /// plain child chain, everything between the two being covered.
    pub(super) slots: [OptIndex; EXACT],
    /// How many of `slots` are in use, or [`Repr::SPAN`].
    pub(super) len: u32,
}

impl Repr {
    /// The tag that says `slots[0]` and `slots[1]` bound an inclusive run.
    pub(super) const SPAN: u32 = u32::MAX;

    /// Nothing marked.
    pub(super) const EMPTY: Self = Self {
        slots: [OptIndex::NONE; EXACT],
        len: 0,
    };
}

plain_data!(Repr);
