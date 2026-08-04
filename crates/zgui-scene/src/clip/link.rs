//! One link of a clip chain.

use zgui_atlas::AtlasTile;
use zgui_geom::{Corners, Device, DevicePx, Rect, Size, Vec2};

use crate::content::{Content, ContentHash};
use crate::id::ClipId;
use crate::spatial::SpatialId;

/// Where a clipping mask's shape came from.
///
/// It decides one thing, and the decision is worth a whole rasterisation pass: a mask that is still
/// a path can be drawn *inside* a vector rasteriser's own scene, and a mask that has already become
/// a raster tile cannot, so an item carrying one has to be composited through a pass of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaskSource {
    /// The mask is a path, and any rasteriser can apply it as a clip layer of its own.
    Path,
    /// The mask is a sampled raster tile — a bitmap `mask-image` — and can only be applied by
    /// something that can sample a texture.
    Raster,
}

/// One clipping test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipLink {
    /// An axis-aligned rectangle with optional elliptical corner radii.
    ///
    /// Radii are a pair per corner because CSS says so: `border-radius: 20px / 10px` is an ellipse
    /// quadrant, and a single scalar per corner cannot express it.
    RoundedRect {
        /// The rectangle content is kept inside.
        rect: Rect<DevicePx, Device>,
        /// The elliptical radii of its four corners.
        radii: Corners<Vec2<DevicePx>>,
        /// The coordinate system the rectangle is measured in.
        ///
        /// A clipping box inside a transformed subtree measures its rectangle in its own space,
        /// while everything that applies a clip — the shader's per-pixel test, the insert cull, a
        /// replay's bounds check — asks in device pixels. The name is carried rather than the
        /// resolved rectangle because the space *moves*: a dialog holding the placement its
        /// entrance settled on is moved by writing its coordinate system, without this link ever
        /// being interned again, so only a resolution against the frame's own matrices can say
        /// where the clip is now. [`SpatialId::VIEWPORT`] — the identity — is every clip outside a
        /// transform, which is nearly all of them.
        space: SpatialId,
    },
    /// An arbitrary shape, rasterised into a coverage tile and sampled.
    Mask {
        /// The coverage tile.
        tile: AtlasTile,
        /// How the tile maps onto the content.
        transform: SpatialId,
        /// Whether the shape is still a path, which decides where the clip can be applied.
        source: MaskSource,
    },
}

impl ClipLink {
    /// A square-cornered rectangular clip, measured in device pixels.
    pub fn rect(rect: Rect<DevicePx, Device>) -> Self {
        Self::RoundedRect {
            rect,
            radii: Corners::uniform(Vec2::splat(DevicePx(0.0))),
            space: SpatialId::VIEWPORT,
        }
    }

    /// A rectangular clip with the same elliptical radii on all four corners, in device pixels.
    pub fn rounded(rect: Rect<DevicePx, Device>, radius: Vec2<DevicePx>) -> Self {
        Self::RoundedRect {
            rect,
            radii: Corners::uniform(radius),
            space: SpatialId::VIEWPORT,
        }
    }

    /// Whether the link needs a rounded-corner test, rather than only narrowing a rectangle.
    pub fn is_rounded(&self) -> bool {
        match self {
            Self::RoundedRect { radii, .. } => [
                radii.top_left,
                radii.top_right,
                radii.bottom_right,
                radii.bottom_left,
            ]
            .iter()
            .any(|radius| radius.x.0 != 0.0 || radius.y.0 != 0.0),
            Self::Mask { .. } => false,
        }
    }

    /// The link as it would be with `shift` taken back out of it.
    ///
    /// A rectangle is stored where it is being drawn, and a clipping box that is being scrolled is
    /// drawn somewhere else on every frame of the scroll. Taking the shift back out asks the
    /// question the other way round — which rectangle of the *unscrolled* document this is — and
    /// that answer holds still while the scroll runs.
    pub(crate) fn unshifted(self, shift: Size<DevicePx, Device>) -> Self {
        match self {
            Self::RoundedRect { rect, radii, space } => Self::RoundedRect {
                rect: rect.translate(Size::new(
                    DevicePx(-shift.width.0),
                    DevicePx(-shift.height.0),
                )),
                radii,
                space,
            },
            other => other,
        }
    }

    /// Whether a vector rasteriser can apply this link inside its own scene.
    ///
    /// Everything expressible is absorbed into the vector content that carries it, which costs a
    /// clip layer; everything else costs a whole rasterisation pass. That is the entire reason this
    /// distinction is on the link rather than being re-derived by each consumer.
    pub fn is_expressible_in_vector_scene(&self) -> bool {
        match self {
            Self::RoundedRect { .. } => true,
            Self::Mask { source, .. } => *source == MaskSource::Path,
        }
    }
}

impl Content for ClipLink {
    fn content_hash(&self) -> u64 {
        let hash = ContentHash::new();
        match self {
            Self::RoundedRect { rect, radii, space } => hash
                .u32(0)
                .f32s(&[
                    rect.origin.x.0,
                    rect.origin.y.0,
                    rect.size.width.0,
                    rect.size.height.0,
                ])
                .f32s(&[
                    radii.top_left.x.0,
                    radii.top_left.y.0,
                    radii.top_right.x.0,
                    radii.top_right.y.0,
                    radii.bottom_right.x.0,
                    radii.bottom_right.y.0,
                    radii.bottom_left.x.0,
                    radii.bottom_left.y.0,
                ])
                // The space is part of what the link *is*: the same rectangle measured in two
                // coordinate systems admits two different sets of pixels. It hashes by name and
                // not by matrix, for the same reason it is carried by name — the chain a moving
                // box issues while it moves has to be the chain it issued before it started.
                .u32(space.index())
                .u32(u32::from(space.generation().get()))
                .finish(),
            Self::Mask {
                tile,
                transform,
                source,
            } => hash
                .u32(1)
                .u64(tile.texture.index as u64)
                .u32(tile.tile.0)
                .i32(tile.bounds.origin.x)
                .i32(tile.bounds.origin.y)
                .i32(tile.bounds.size.width)
                .i32(tile.bounds.size.height)
                .u32(transform.index())
                .u32(u32::from(transform.generation().get()))
                .u32(u32::from(*source == MaskSource::Raster))
                .finish(),
        }
    }
}

/// A node of the clip trie: one link on top of a shorter chain.
///
/// Chains are stored innermost-first, each pointing at its parent, so two chains that share an
/// outer prefix share the same nodes. That is what makes "the deepest clip these items have in
/// common" a walk up two parent pointers rather than a comparison of two lists.
///
/// # What makes two nodes the same node
///
/// The rectangle a node holds is where its clipping box is being drawn, and a box inside something
/// that is scrolling is drawn somewhere else on every frame of the scroll. Identity is taken on the
/// rectangle with that movement subtracted — the same box, wherever the document has been scrolled
/// to — so a chain interned again as the scroll runs is the chain that was already there, holding
/// where its box has moved to. Deciding it on the drawn rectangle instead would mint a node per
/// clipping box per frame, and the table would grow for as long as anything scrolled.
///
/// Everything that *reads* a node reads the rectangle as drawn, because that is what a clip is
/// applied at.
#[derive(Clone, Copy, Debug)]
pub enum ClipNode {
    /// The chain that clips nothing.
    Root,
    /// One more link, applied inside everything `parent` already applies.
    Link {
        /// The test this node adds, where it is being drawn.
        link: ClipLink,
        /// The chain it is applied inside.
        parent: ClipId,
        /// How far the scroll and sticky offsets above the clipping box have carried `link` from
        /// where layout placed it.
        shift: Size<DevicePx, Device>,
    },
}

impl ClipNode {
    /// The link this node adds, as layout placed it rather than as it is being drawn.
    fn settled(&self) -> Option<ClipLink> {
        match self {
            Self::Root => None,
            Self::Link { link, shift, .. } => Some(link.unshifted(*shift)),
        }
    }
}

impl PartialEq for ClipNode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Root, Self::Root) => true,
            (
                Self::Link { parent, .. },
                Self::Link {
                    parent: other_parent,
                    ..
                },
            ) => parent == other_parent && self.settled() == other.settled(),
            _ => false,
        }
    }
}

impl Content for ClipNode {
    fn content_hash(&self) -> u64 {
        match self {
            Self::Root => ContentHash::new().u32(u32::MAX).finish(),
            Self::Link { parent, .. } => ContentHash::new()
                .u64(self.settled().map_or(0, |link| link.content_hash()))
                .u32(parent.0)
                .finish(),
        }
    }

    fn same_stored_value(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Root, Self::Root) => true,
            (
                Self::Link {
                    link,
                    parent,
                    shift,
                },
                Self::Link {
                    link: other_link,
                    parent: other_parent,
                    shift: other_shift,
                },
            ) => link == other_link && parent == other_parent && shift == other_shift,
            _ => false,
        }
    }
}
