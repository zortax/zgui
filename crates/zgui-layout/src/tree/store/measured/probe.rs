//! The question one size-only measurement answered.

use taffy::{AvailableSpace, LayoutInput, RequestedAxis, Size, SizingMode};

/// What one axis of a box was constrained to when it was measured.
///
/// The four cases are kept apart by their own discriminant rather than by packing them into one
/// number. Packing is what the engine's own key does, and it is what makes a definite space of
/// exactly infinity indistinguishable from the min-content keyword — a difference that no test of
/// ordinary documents would ever produce and that would answer one question with the other's size
/// if one did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Constraint {
    /// The dimension was already decided, by its bits.
    Known(u32),
    /// It was not, and this much space was available, by its bits.
    Definite(u32),
    /// It was not, and the box was to be as narrow as its content allows.
    MinContent,
    /// It was not, and the box was to be as wide as it likes.
    MaxContent,
}

impl Constraint {
    /// The constraint one axis was under.
    ///
    /// A known dimension wins: an axis whose size is already decided ignores the space available on
    /// it, so two probes that differ only there are the same question.
    fn of(known: Option<f32>, available: AvailableSpace) -> Self {
        match (known, available) {
            (Some(size), _) => Self::Known(size.to_bits()),
            (None, AvailableSpace::Definite(space)) => Self::Definite(space.to_bits()),
            (None, AvailableSpace::MinContent) => Self::MinContent,
            (None, AvailableSpace::MaxContent) => Self::MaxContent,
        }
    }
}

/// Everything about a size-only measurement that can change its answer.
///
/// Compared by bits and never by value. Two constraints that are not the same number must never be
/// taken for one, and a floating-point comparison cannot promise that: it makes two `NaN`s unequal,
/// so a degenerate constraint would miss for ever rather than being answered once, and it makes a
/// positive and a negative zero equal, which they are not as an available space.
///
/// The fields are the whole of the engine's own question, field for field. The two constraints are
/// what the box was given; `parent` is the containing block percentages inside it resolve against;
/// `axis` is there because a measurement is taken of one axis at a time; `sizing` because it decides
/// whether the box's own sizing styles take part in the answer at all — a probe taken with them
/// ignored is a different question from one taken with them applied, and both are asked of the same
/// box in the same pass; and `collapsible` because a block box whose vertical margins collapse into
/// its parent's is a block box of a different height.
///
/// That last one is carried even though the algorithms in use ask every size-only question with it
/// unset, and the reason is what this cache is for. The engine's own nine slots are keyed on part
/// of the question and this is keyed on the whole of it, which is the only thing that makes it
/// sound to consult after the engine's has missed. A field left out because nothing currently
/// varies it is a claim about the *caller* rather than about the question, and the day something
/// varies it the memo answers one question with another's size — with no miss, no assertion and no
/// symptom beyond a box of the wrong height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Probe {
    /// What the inline axis was constrained to.
    width: Constraint,
    /// What the block axis was constrained to.
    height: Constraint,
    /// The containing block, by its bits, where it is definite.
    parent: (Option<u32>, Option<u32>),
    /// Which axis was asked about.
    axis: u8,
    /// Whether the box's own sizing styles took part.
    sizing: u8,
    /// Whether the box's vertical margins were allowed to collapse into its parent's, per edge.
    collapsible: (bool, bool),
}

impl Probe {
    /// The question one layout input asks.
    pub(crate) fn of(input: &LayoutInput) -> Self {
        Self {
            width: Constraint::of(input.known_dimensions.width, input.available_space.width),
            height: Constraint::of(input.known_dimensions.height, input.available_space.height),
            parent: (
                input.parent_size.width.map(f32::to_bits),
                input.parent_size.height.map(f32::to_bits),
            ),
            axis: match input.axis {
                RequestedAxis::Horizontal => 0,
                RequestedAxis::Vertical => 1,
                RequestedAxis::Both => 2,
            },
            sizing: match input.sizing_mode {
                SizingMode::ContentSize => 0,
                SizingMode::InherentSize => 1,
            },
            collapsible: (
                input.vertical_margins_are_collapsible.start,
                input.vertical_margins_are_collapsible.end,
            ),
        }
    }
}

/// The size a probe was answered with.
pub(crate) type Answer = Size<f32>;
