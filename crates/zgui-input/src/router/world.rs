//! Everything about the frame an event is being routed against.

use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_geom::{Css, Device, DevicePx, Point, Scale};
use zgui_layout::{HitIndex, LayoutStore};
use zgui_scene::{ClipTable, SpatialTree};
use zgui_vocab::UiState;

use crate::hit::HitChain;

/// The document, its geometry, and what it takes to ask questions of both.
///
/// Assembled once per event by whatever drives the frame and handed to the router, which holds
/// none of it: every field here belongs to the frame that produced it, and a router that kept one
/// would be holding last frame's geometry the moment anything moved.
pub struct World<'a> {
    /// The document being interacted with.
    pub document: &'a Document,
    /// Its boxes and fragments, as of the last completed frame.
    pub layout: &'a LayoutStore,
    /// What is under a point.
    pub hit: &'a HitIndex,
    /// The clip chains the fragments were measured against.
    pub clips: &'a ClipTable,
    /// The coordinate systems they were measured in.
    ///
    /// The tree rather than a copy of its matrices: a coordinate system is named by the box that
    /// establishes it, so the name a fragment carries goes on meaning the same coordinate system,
    /// and what an event needs is where that coordinate system is *now*.
    pub spatial: &'a SpatialTree,
    /// How many device pixels one CSS pixel is.
    pub scale: Scale<Css, Device>,
    /// Which changes can affect a computed style, so a state write nothing matches costs nothing.
    pub filter: &'a dyn StyleFilter,
}

impl core::fmt::Debug for World<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("World")
            .field("nodes", &self.document.len())
            .field("fragments", &self.hit.len())
            .field("scale", &self.scale.get())
            .finish_non_exhaustive()
    }
}

impl World<'_> {
    /// The path from the document's root down to whatever is under `point`.
    ///
    /// Empty when the point is over nothing at all.
    pub fn chain_at(&self, point: Point<DevicePx, Device>) -> HitChain {
        crate::hit::at(
            self.document.store(),
            self.layout,
            self.hit,
            self.clips,
            self.spatial,
            point,
        )
        .map(|hit| hit.chain)
        .unwrap_or_default()
    }

    /// The scrollbar under `point`, if the topmost thing there is one.
    pub fn scrollbar_at(
        &self,
        point: Point<DevicePx, Device>,
    ) -> Option<crate::hit::ScrollbarPress> {
        crate::hit::scrollbar::at(self.layout, self.hit, self.clips, self.spatial, point)
    }

    /// The path holding only the document's root element.
    ///
    /// Where a key goes when nothing has focus, and it is one element long: a listener on the root
    /// hears it and a listener anywhere below the root does not. Widening it is not the answer,
    /// because nothing in the tree says which of the listeners further down wanted an unfocused
    /// key — so a longer path would hand one to every list's type-ahead and every editor's
    /// bindings. What hears a key nobody has focus for is named instead, and delivered to after
    /// this path rather than found on it.
    pub fn root_chain(&self) -> HitChain {
        match self.document.root_index() {
            Some(root) => HitChain::from_path([self.document.store().key_of(root)]),
            None => HitChain::default(),
        }
    }

    /// The interaction state of one element, as the style engine will match it.
    pub fn state_of(&self, node: NodeKey) -> UiState {
        match self.document.store().index_of(node) {
            Some(index) => self.document.store().core(index).ui_state(),
            None => UiState::EMPTY,
        }
    }
}
