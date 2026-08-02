//! Grid templates, track sizes and item placement.
//!
//! A track list is a flat sequence in which a `repeat()` is one entry, and the line names sit in a
//! parallel sequence one longer than it — so a reader walks the two together rather than looking
//! for names inside a track.

/// A named grid line or area.
pub use style::values::CustomIdent;
/// The computed value of `grid-auto-flow`, a set of bit flags rather than an enumeration.
pub use style::values::computed::GridAutoFlow as GridAutoFlowValue;
/// The computed value of one end of `grid-row` or `grid-column`.
pub use style::values::computed::GridLine as GridLineValue;
/// The computed value of `grid-template-areas`.
pub use style::values::computed::GridTemplateAreas as GridTemplateAreasValue;
/// The computed value of `grid-template-rows` and `grid-template-columns`.
pub use style::values::computed::GridTemplateComponent as GridTemplateComponentValue;
/// The computed value of `grid-auto-rows` and `grid-auto-columns`.
pub use style::values::computed::ImplicitGridTracks as ImplicitGridTracksValue;
/// One track's minimum or maximum sizing function.
pub use style::values::computed::TrackBreadth as TrackBreadthValue;
/// The list of tracks inside a `grid-template-*` value, with its parallel line names.
pub use style::values::computed::TrackList as TrackListValue;
/// One track's sizing function: a breadth, a `minmax()` or a `fit-content()`.
pub use style::values::computed::TrackSize as TrackSizeValue;
/// A flex fraction: the `fr` unit, which distributes what is left after every other track.
pub use style::values::generics::grid::Flex;
/// How many times a `repeat()` repeats: a number, `auto-fill` or `auto-fit`.
pub use style::values::generics::grid::RepeatCount;
/// One entry of a track list: either a single track or a `repeat()`.
pub use style::values::generics::grid::TrackListValue as TrackListEntry;
/// A `repeat()` clause, with its count, its tracks and its own parallel line names.
pub use style::values::generics::grid::TrackRepeat as TrackRepeatValue;
/// One rectangle of `grid-template-areas`, in one-based grid line numbers.
pub use style::values::specified::position::NamedArea;
