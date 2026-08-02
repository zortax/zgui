//! Which counters mean anything under which renderer, and which of them count avoided work.

use crate::counter::Counter;

/// What kind of quantity a counter records.
///
/// Two questions are answered here because both are properties of the counter itself and both are
/// the kind of thing that goes quietly wrong when it is written down somewhere else.
///
/// The first is whether the count depends on the renderer behind it. A test harness that swaps the
/// real renderer for a capture stub can assert on anything that is not [`Group::RendererSpecific`]
/// and must ignore every counter that is, since those would simply read zero. Stating that per
/// counter is the difference between a suite that means what it says and one whose assertions
/// quietly stop testing anything.
///
/// The second is whether the counter is the *skipped* half of a stage that avoids work. A counter
/// of avoided work is the one kind of counter that reads zero when everything is broken and zero
/// when everything is perfect, so a single such counter satisfies any assertion written about it.
/// [`Group::Skip`] names the counter of the work that was actually done instead, so the two are
/// always read together, and it is what `cargo xtask skips` searches for when it looks for the
/// non-vacuity assertion the pair is required to have.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Group {
    /// Incremented by a stage that does not know or care what draws the result, so the count is
    /// the same under any renderer, including one that draws nothing.
    ///
    /// Some counters are in this group deliberately even though they name a rendering concept:
    /// what they record is a decision the scene stage made, not work a renderer did, so the count
    /// is a property of the scene and asserting it asserts something real.
    BackendNeutral,

    /// Incremented by the GPU renderer, so it reads zero under a renderer that does not submit
    /// work, and an assertion on it is an assertion about that particular backend.
    RendererSpecific,

    /// Backend-neutral, and counts work a stage *did not do* because it could reuse an answer.
    ///
    /// The counter of the work that was done instead is named here, because neither number means
    /// anything alone: a stage that skipped nine tenths of its work and a stage that was never
    /// asked to do any both leave the skip counter looking healthy relative to nothing.
    Skip {
        /// The counter of the work the same stage performed rather than skipped.
        done: Counter,
    },

    /// Backend-neutral, and holds how many of something are alive *right now* rather than how many
    /// were ever made.
    ///
    /// A live count is written with [`set`](crate::counter::set) and not with
    /// [`add`](crate::counter::add), because it is a gauge: the producer publishes the length of
    /// the thing it owns at the end of every frame, and the value replaces whatever was there.
    ///
    /// The reason the distinction is worth a group of its own is that a gauge is the only counter
    /// shape that can express *growth*. A total that rises is a program doing work; a live count
    /// that rises across a thousand identical ticks is a program that never gave something back,
    /// and that is a defect no accumulating counter can state. Everything in this group is read as
    /// a pair — its value early in a run and its value late in the same run — and the two are
    /// required to be equal.
    Live,
}

impl Group {
    /// A short name for the group.
    pub const fn label(self) -> &'static str {
        match self {
            Self::BackendNeutral => "backend-neutral",
            Self::RendererSpecific => "renderer-specific",
            Self::Skip { .. } => "skip",
            Self::Live => "live",
        }
    }

    /// Whether the count is the same under any renderer, including one that draws nothing.
    ///
    /// A skip counter is backend-neutral: the two axes are independent, and a stage that decided
    /// to reuse an answer decided it whatever was going to draw the result.
    pub const fn is_backend_neutral(self) -> bool {
        !matches!(self, Self::RendererSpecific)
    }

    /// The counter of work performed that this one's avoided work must be read against.
    pub const fn done(self) -> Option<Counter> {
        match self {
            Self::Skip { done } => Some(done),
            Self::BackendNeutral | Self::RendererSpecific | Self::Live => None,
        }
    }

    /// Whether the counter is a gauge read as a pair of samples rather than a total.
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Live)
    }
}
