//! The identifiers primitives, fragments and side tables address each other by.
//!
//! Every one is a `u32` handle. They are separate types rather than one, because the whole point
//! is that a paint index cannot be passed where a clip index is expected — the failure mode being
//! a fragment drawn with a stranger's colour and no error anywhere.
//!
//! The layout tree stores some of them on every fragment, which is why it depends on this crate: a
//! second, parallel id space in layout would be the same aliasing problem one level up.

/// A primitive's position in the painting order.
///
/// Higher draws later, so higher wins where two primitives overlap. Two primitives at *equal*
/// order are provably non-overlapping — [`BoundsTree`](crate::BoundsTree) is what makes that true —
/// and at equal order the primitive kind decides the sequence, purely so that a batch of one kind
/// stays contiguous.
pub type DrawOrder = u32;

/// Declares a `u32` handle type with a documented meaning.
macro_rules! handle {
    ($name:ident, $($doc:literal),+ $(,)?) => {
        $(#[doc = $doc])+
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);

        impl $name {
            /// The handle's numeric value, for indexing and for transcripts.
            pub const fn index(self) -> u32 {
                self.0
            }
        }
    };
}

handle!(
    ClipId,
    "A chain of clip links, resolved through a [`ClipTable`](crate::ClipTable).",
    "",
    "A chain rather than a rectangle, because CSS nests clipping: a rounded card inside a",
    "scrollport inside a `clip-path` is three links, and truncating the chain at a fixed depth is",
    "a correctness cliff rather than an optimisation. [`ClipId::ROOT`] is the chain that clips",
    "nothing.",
);

handle!(
    PaintId,
    "A fill or a stroke source, resolved through a [`PaintTable`](crate::PaintTable).",
    "",
    "Paint lives in a table rather than in the instance so that a gradient with fifty stops costs",
    "a primitive exactly what a flat colour does.",
);

handle!(
    PaintSlot,
    "A text brush, resolved through a [`TextPaintTable`](crate::TextPaintTable).",
    "",
    "Separate from [`PaintId`] because it is the one paint that is **mutated in place**: a shaped",
    "paragraph holds this index, and re-colouring every cached paragraph on a theme change is a",
    "write through the slot rather than a re-shape. Content interning would make that mutation",
    "unsound, since two paragraphs that computed to the same colour by different routes would",
    "share a slot and one would be re-coloured by the other's theme.",
);

handle!(
    StackingContextId,
    "One stacking context, as CSS defines them.",
    "",
    "Fragments carry it so that painting order and hit testing agree about which context a",
    "fragment belongs to.",
);

handle!(
    ScrollFrameId,
    "One scrollable region.",
    "",
    "Carried by fragments inside a scrollport, so that a scroll can be serviced by translating",
    "recorded paint operations rather than by re-emitting them.",
);

handle!(
    VectorId,
    "One piece of vector content, stable across frames.",
    "",
    "A rasteriser keeps its own encoded form of the geometry under this id and re-places it each",
    "frame instead of re-encoding it, so this crate never has to know what that encoded form is.",
);

impl ScrollFrameId {
    /// The window itself.
    ///
    /// A scrollable region is named after the box that establishes one, and the window is not a
    /// box — but it scrolls, and a `position: sticky` box with nothing scrollable above it is held
    /// still against *it*. So it is named after the slot number no box reaches, for the same reason
    /// and with the same margin as any other reserved handle.
    pub const VIEWPORT: Self = Self(u32::MAX);
}

impl ClipId {
    /// The chain that clips nothing, which every other chain descends from.
    pub const ROOT: Self = Self(0);

    /// Whether this is the chain that clips nothing.
    pub const fn is_root(self) -> bool {
        self.0 == Self::ROOT.0
    }
}
