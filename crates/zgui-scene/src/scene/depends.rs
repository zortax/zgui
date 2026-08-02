//! Whether every primitive is still drawn through the coordinate system it was pushed under.
//!
//! # What can go wrong, and why nothing else sees it
//!
//! A primitive carries the *slot* of its coordinate system and nothing else: four bytes a shader
//! indexes a dense array with. A slot is reused. A box goes away, its node is given back, the slot
//! comes back to the allocator and the next box to establish a coordinate system is handed it —
//! and every primitive still carrying that number is now drawn through a stranger's matrix.
//!
//! The reason this needs a check of its own is that it produces no symptom anything else in the
//! project can detect. The primitive's own bytes are unchanged, so a transcript prints what it
//! printed. Its rectangle is unchanged, so every border box agrees. The matrix it resolves to is a
//! real matrix belonging to a real box, so the pixels are plausible and a damage comparison against
//! a window built from nothing agrees — that window's slot is occupied by the same stranger.
//!
//! # Why the name is recorded and not recomputed
//!
//! Asking whether the slot a primitive names is occupied *now* answers yes in exactly the case
//! that is wrong, because the stranger occupies it. What has to be compared is the name the
//! primitive was pushed under — slot **and occupancy counter** — against the name the slot holds
//! now, and the counter exists for nothing else. So the name is kept beside the log entry as the
//! primitive is pushed, and a replayed range carries the name it was recorded with rather than
//! being renamed on the way through.
//!
//! Kept only when [`invariant::enabled`](crate::invariant::enabled) says so: it is a word per
//! primitive per frame, which is the wrong price for a window that is merely running.

use core::fmt;

use crate::prim::PrimitiveKind;
use crate::scene::Scene;
use crate::spatial::SpatialId;

/// One primitive drawn through a coordinate system that is not the one it was pushed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpatialFault {
    /// Where in this frame's log the primitive is.
    pub op: u32,
    /// What kind of primitive it is.
    pub kind: PrimitiveKind,
    /// The name it was pushed under.
    pub named: SpatialId,
    /// The name that slot holds now, which is `None` where nothing holds it.
    pub holding: Option<SpatialId>,
}

impl fmt::Display for SpatialFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "operation {} ({:?}) is drawn through slot {}, which it was pushed under as occupant \
             {} and is held now by ",
            self.op,
            self.kind,
            self.named.index(),
            self.named.generation().get(),
        )?;
        match self.holding {
            Some(holding) if holding.index() == self.named.index() => {
                write!(f, "occupant {}", holding.generation().get())
            }
            Some(holding) => write!(f, "the unrelated name {holding:?}"),
            None => write!(f, "nothing at all"),
        }
    }
}

impl Scene {
    /// Keeps the name every primitive is pushed under, so that there is something to compare.
    ///
    /// On already for a scene made while `ZGUI_INVARIANTS` is set, which is how an application asks
    /// for it. This is the same switch reached directly, for a caller that is checking one scene
    /// rather than a whole run.
    ///
    /// # Panics
    ///
    /// If the frame has already had something pushed into it. The names are kept alongside the log
    /// and are meaningless unless they have been kept since the log began.
    pub fn record_spatial_dependencies(&mut self, record: bool) {
        assert!(
            self.ops.is_empty(),
            "the names are recorded as primitives are pushed, so this belongs before the pushing",
        );
        self.checking = record;
        if !record {
            self.spaces.clear();
            self.retained_spaces.clear();
        }
    }

    /// How many of this frame's primitives carry the name of a coordinate system.
    ///
    /// The non-vacuity control for the check below, and the reason it is worth having is that the
    /// check's green and its vacuous green look identical: a frame that recorded nothing has
    /// nothing to compare and reports no faults, which is also what an intact frame reports. Zero
    /// here on a frame that drew something is a check that has been switched off rather than a
    /// check that passed.
    pub fn spatial_dependencies_recorded(&self) -> usize {
        self.spaces.iter().filter(|named| named.is_some()).count()
    }

    /// Every primitive whose coordinate system is no longer the one it was pushed under.
    ///
    /// Empty for a frame that is intact, and empty for every frame when the checks are switched
    /// off, because nothing is recorded to compare against then.
    ///
    /// What is compared is the whole name and not the slot in it. A slot that has come back
    /// resolves perfectly well — to a stranger — so a check reading the primitive's four bytes and
    /// asking whether they resolve reports nothing in exactly the case that is wrong. It is the
    /// occupancy counter, recorded when the primitive was pushed and carried through a replay, that
    /// tells the two occupants apart.
    ///
    /// A coordinate system whose *matrix* changed is not a fault and must not become one: a
    /// structural name is the same name while the box it belongs to moves, and that is the whole of
    /// what the representation buys.
    pub fn spatial_faults(&self) -> Vec<SpatialFault> {
        let mut faults = Vec::new();
        for (op, named) in self.spaces.iter().enumerate() {
            let Some(named) = *named else {
                continue;
            };
            let holding = self.spatial.at(named.index());
            if holding != Some(named) {
                faults.push(SpatialFault {
                    op: op as u32,
                    kind: self.ops[op].kind,
                    named,
                    holding,
                });
            }
        }
        faults
    }

    /// Panics if any primitive is drawn through a coordinate system that changed hands under it.
    ///
    /// The panic is the point. What follows this frame is a renderer indexing a dense array of
    /// matrices with numbers the display list carries, and the picture it produces is a plausible
    /// one: the failure it shows says nothing whatever about the cause.
    ///
    /// # Panics
    ///
    /// If any primitive names a coordinate system that is not the one it was pushed under.
    pub fn check_spatial_dependencies(&self) {
        if !self.checking {
            return;
        }
        let faults = self.spatial_faults();
        assert!(
            faults.is_empty(),
            "primitives drawn through coordinate systems that changed hands under them: {}",
            faults
                .iter()
                .map(SpatialFault::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
}

#[cfg(test)]
mod tests;
