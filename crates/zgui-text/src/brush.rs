//! The brush a shaped run carries.

/// What a run of glyphs is drawn with: an index into a table the document owns, never a colour.
///
/// A shaped paragraph is expensive and outlives the frame that produced it. If it stored a colour,
/// switching theme would invalidate every one of them, and a dark-mode toggle would re-shape every
/// string in the application. Storing an index instead makes the same toggle a handful of writes
/// into the table, with no shaped result touched and no cache emptied.
///
/// The table entries have to be stable for as long as a current context names them, which is why
/// the table lives across frames rather than being rebuilt each frame: a paragraph replayed from a
/// previous frame would otherwise be drawn in whatever colour landed in its slot next. The owner
/// releases a slot only when no element is drawn through it and no key resolves to it. The brush is
/// part of the paragraph key, so a cached result that names a released slot serves only a context
/// that names the same slot again.
pub type Brush = zgui_scene::PaintSlot;
