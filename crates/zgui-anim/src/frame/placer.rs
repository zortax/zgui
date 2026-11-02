//! Who moves or recolours the box, when that can be done without composing it.

use zgui_dom::NodeKey;
use zgui_dom::side::{AnimOverride, AnimPlacement};

use crate::frame::apply::Placed;

/// What a tick offers each moved placement to, before anything is marked for it.
///
/// The split is the point. *Deciding* that an element is on the placement path is this crate's, and
/// it is decided from the properties alone; *moving* the box is not, because where a box's
/// coordinate system lives and whether writing it is safe are facts about the display list, which
/// this crate names nowhere and must go on not naming.
pub trait Placer {
    /// Moves `node`'s box to `placement`, and reports whether it could.
    ///
    /// Called only for an element whose placement differs from the one the standing fragments were
    /// composed under, so an implementation is never asked to move a box that is already there.
    fn place(&mut self, node: NodeKey, placement: &AnimPlacement) -> Placed;

    /// Reports that `node` is no longer being placed at all.
    ///
    /// Its animation ended, so the box goes back to the transform its own style asks for and
    /// anything an implementation was holding *about the movement* — where it was going, what it
    /// was ordered against — is about a movement that is over. The default does nothing, which is
    /// right for an implementation that was holding nothing.
    fn retired(&mut self, node: NodeKey) {
        let _ = node;
    }

    /// Writes `node`'s animated colours into the paint slots its box owns, and reports whether
    /// that is all the frame owes the box.
    ///
    /// Called for an element whose override changed. `true` leaves the element owing nothing for
    /// its colours: the implementation rewrote every slot the box's recorded painting names and
    /// damaged the box's ink itself. `false` marks the repaint, which is always correct. The
    /// default answers `false`.
    fn recolor(&mut self, node: NodeKey, values: &AnimOverride) -> bool {
        let _ = (node, values);
        false
    }

    /// Reports that `node` no longer carries the override `was`, so its box is drawn from its own
    /// style again.
    ///
    /// The repaint that is owed for it is marked as well, so an implementation that does nothing
    /// is correct for a box the fragment pass damages. The default does nothing.
    fn uncolored(&mut self, node: NodeKey, was: &AnimOverride) {
        let _ = (node, was);
    }
}

/// The placer for a caller with nowhere to write.
///
/// Every placement is answered [`Placed::Recomposed`], which is what a transform cost before there
/// was anywhere to write it and what an interactive one costs still. It is also what a test of this
/// crate alone uses, because a tick with no display list behind it has no coordinate system to move.
#[derive(Clone, Copy, Debug, Default)]
pub struct Recomposing;

impl Placer for Recomposing {
    fn place(&mut self, _node: NodeKey, _placement: &AnimPlacement) -> Placed {
        Placed::Recomposed
    }
}
