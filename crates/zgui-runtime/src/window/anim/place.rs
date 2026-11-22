//! Moving an animated box by writing the coordinate system it established.
//!
//! What a transform used to cost was the box being composed again: its fragments rebuilt from the
//! new matrix, its primitives emitted again, its hit entry rewritten. What it costs here is one
//! matrix written into a node the box already owns. The fragments below it are in their own
//! untransformed space and the hit entries under them are indexed in that space, so nothing else
//! changes, and the compositor moves what the box recorded.
//!
//! A write moves only what the box recorded, and the paint walk records only what is on the
//! surface. So a box is written only while all of its ink is on the surface, and a box that is
//! still partly off it is composed again at its new place.

use zgui_anim::{Placed, Placer};
use zgui_bits::DamageSet;
use zgui_dom::NodeKey;
use zgui_dom::side::{AnimOverride, AnimPlacement};
use zgui_geom::DevicePx;
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::Painter;
use zgui_paint::Tables;
use zgui_profile::{Counter, counter};
use zgui_space::{PropertyOwner, SpatialId};

/// Moves boxes by writing the display list's coordinate systems, and recolours them by writing
/// the paint slots they own.
pub(crate) struct Writing<'a> {
    /// The display list holding the coordinate systems and the frame that is on the screen.
    pub tables: &'a mut Tables,
    /// The boxes, for the border box a transform is resolved against.
    pub layout: &'a LayoutStore,
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
    /// Where every moved box now is, and the ink of every box recoloured, which the frame's paint
    /// walk has to reach.
    pub damage: DamageSet,
    /// The paint records, for which paint slots each box's painting names.
    pub painter: &'a Painter,
    /// The coordinate systems the writes moved.
    pub moved: Vec<SpatialId>,
    /// The surface's extent on the device.
    pub surface: zgui_geom::Size<i32, zgui_geom::Device>,
    /// The clips that admit damage beyond what they show, as
    /// [`ClipTable::bounds_widened`](zgui_space::ClipTable::bounds_widened) takes them.
    pub widened: &'a [(
        zgui_space::PropertyOwner,
        zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
    )],
}

impl Placer for Writing<'_> {
    fn place(&mut self, node: NodeKey, placement: &AnimPlacement) -> Placed {
        let Some((space, matrix)) = self
            .moved_to(node, placement)
            .filter(|(space, _)| self.on_surface(node, *space))
        else {
            counter::bump(Counter::PlaceWritesWithReemit);
            return Placed::Recomposed;
        };
        if self.tables.spatial.place(space, matrix) {
            counter::bump(Counter::PlaceWritesWithoutReemit);
            self.moved.push(space);
            self.reach(node);
        }
        Placed::Written
    }

    fn recolor(&mut self, node: NodeKey, values: &AnimOverride) -> bool {
        self.write_colors(node, values)
    }

    fn uncolored(&mut self, node: NodeKey, was: &AnimOverride) {
        self.damage_uncolored(node, was);
    }
}

impl Writing<'_> {
    /// Damages where the box now is on the device.
    ///
    /// A box carried onto the surface by its matrix may hold fragments no frame has painted, and
    /// the paint walk paints only what the damage reaches.
    fn reach(&mut self, node: NodeKey) {
        let Some(box_key) = self.layout.boxes_of(node).first() else {
            return;
        };
        for frag in self.layout.fragments_of_box(*box_key) {
            if let Some(fragment) = self.layout.fragment(*frag) {
                let ink = fragment.on_device_in(fragment.subtree_ink, &self.tables.spatial);
                self.damage.absorb(whole(ink));
            }
        }
    }

    /// Reports whether all of the ink of `node`'s box lies on the surface where `space` puts it
    /// now.
    fn on_surface(&self, node: NodeKey, space: SpatialId) -> bool {
        let Some(box_key) = self.layout.boxes_of(node).first() else {
            return false;
        };
        let Some(matrix) = self.tables.spatial.resolve(space) else {
            return false;
        };
        let surface = zgui_geom::Rect::new(
            zgui_geom::Point::new(DevicePx(0.0), DevicePx(0.0)),
            zgui_geom::Size::new(
                DevicePx(self.surface.width as f32),
                DevicePx(self.surface.height as f32),
            ),
        );
        self.layout
            .fragments_of_box(*box_key)
            .iter()
            .filter_map(|frag| self.layout.fragment(*frag))
            .all(|fragment| {
                let ink = zgui_geom::transformed_bounds(&matrix, fragment.subtree_ink);
                ink.is_empty() || surface.contains_rect(ink)
            })
    }

    /// The coordinate system an element's box established, and the matrix the placement puts it at.
    ///
    /// `None` for an element with no box, no fragment, no coordinate system of its own, or a
    /// placement that resolves to no transform at all — every one of which is a box the write has
    /// nowhere to go for.
    fn moved_to(&self, node: NodeKey, placement: &AnimPlacement) -> Option<(SpatialId, Matrix)> {
        let box_key = *self.layout.boxes_of(node).first()?;
        let frag = *self.layout.fragments_of_box(box_key).first()?;
        let fragment = self.layout.fragment(frag)?;
        // The same call the fragment pass makes, against the same border box, so the matrix written
        // here and the matrix a composition would arrive at are one value rather than two readings.
        let matrix = zgui_layout::fragment::transform::matrix_of(
            placement.group(),
            fragment.border_box,
            self.scale,
        )?;
        let space = self.tables.spatial.of(PropertyOwner::of(box_key))?;
        Some((space, matrix))
    }
}

/// The matrix a placement resolves to.
type Matrix = zgui_geom::Matrix4;

/// A fractional rectangle grown to the whole device pixels that can show any of it.
fn whole(
    bounds: zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>,
) -> zgui_geom::Rect<i32, zgui_geom::Device> {
    zgui_geom::Rect::from_corners(
        zgui_geom::Point::new(
            bounds.left().0.floor() as i32 - 1,
            bounds.top().0.floor() as i32 - 1,
        ),
        zgui_geom::Point::new(
            bounds.right().0.ceil() as i32 + 1,
            bounds.bottom().0.ceil() as i32 + 1,
        ),
    )
}
