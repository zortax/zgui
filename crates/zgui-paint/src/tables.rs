//! The side tables fragments are painted through.

use zgui_geom::{Device, Size};
use zgui_space::{ClipId, ClipTable, SpatialTree};

use crate::paints::PaintTable;

/// The clip chains, paints and coordinate systems of a document, kept across frames.
///
/// Ids in each table stay stable while anything holds them, so a replayed fragment resolves the
/// ids it was painted with.
#[derive(Debug)]
pub struct Tables {
    /// The clip chains.
    pub clips: ClipTable,
    /// The paint sources.
    pub paints: PaintTable,
    /// The coordinate systems.
    pub spatial: SpatialTree,
    /// The surface extent of the frame being painted.
    viewport: Size<i32, Device>,
    /// The clip chains minted since [`Tables::begin_minting`], while an encoding is open.
    minted: Option<Vec<ClipId>>,
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}

impl Tables {
    /// Returns tables that hold only the root clip and the viewport's coordinate system.
    pub fn new() -> Self {
        Self {
            clips: ClipTable::rooted(),
            paints: PaintTable::new(),
            spatial: SpatialTree::with_viewport(),
            viewport: Size::new(0, 0),
            minted: None,
        }
    }

    /// Starts a frame painted onto a surface of `viewport`.
    ///
    /// The clip and paint tables start a new generation, which their eviction counts entries
    /// unused by.
    pub fn begin_frame(&mut self, viewport: Size<i32, Device>) {
        self.viewport = viewport;
        self.clips.begin_frame();
        self.paints.begin_frame();
    }

    /// Returns the surface extent of the frame being painted.
    pub fn viewport(&self) -> Size<i32, Device> {
        self.viewport
    }

    /// Starts collecting the clip chains an encoding mints for its own content.
    pub fn begin_minting(&mut self) {
        self.minted = Some(Vec::new());
    }

    /// Notes a clip chain the open encoding minted.
    ///
    /// Nothing is noted while no encoding is open.
    pub fn note_minted_clip(&mut self, id: ClipId) {
        if let Some(minted) = &mut self.minted {
            minted.push(id);
        }
    }

    /// Stops collecting and returns the clip chains minted since [`Tables::begin_minting`].
    pub fn take_minted(&mut self) -> Vec<ClipId> {
        self.minted.take().unwrap_or_default()
    }
}
