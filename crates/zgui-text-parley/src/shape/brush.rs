//! The brush a shaped run carries, in the shape the engine requires.

use zgui_scene::PaintSlot;
use zgui_text::Brush;

/// A paint slot, wrapped so that it can travel through a shaped layout.
///
/// The engine requires a brush to have a default value; a paint slot deliberately does not, because
/// slot zero is an ordinary entry in the table and "no brush" is not a thing a run can be. The
/// wrapper supplies the default the engine needs without letting that default leak into the paint
/// table's own vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotBrush(pub Brush);

impl Default for SlotBrush {
    fn default() -> Self {
        Self(PaintSlot(0))
    }
}

impl From<Brush> for SlotBrush {
    fn from(slot: Brush) -> Self {
        Self(slot)
    }
}
