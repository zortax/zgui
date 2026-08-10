//! The three sprite kinds, and the atlas tile they read from.

use bytemuck::{Pod, Zeroable};
use zgui_atlas::AtlasTile;
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Rect, Size, Vec2};

use crate::id::{ClipId, DrawOrder};
use crate::prim::layout::rect_of;
use crate::resource::{ResourceGeneration, ResourceKey, ResourceKind};
use crate::spatial::SpatialId;

/// What a sprite samples: either where the pixels are, or what they are called.
///
/// A producer that already knows where a raster landed hands over the placement and the sprite is
/// finished as it is built. One that only knows the content's name hands that over instead, and the
/// sprite carries a placeholder until [`Scene::resolve_resources`](crate::Scene::resolve_resources)
/// fills it in. Nothing downstream of the fix-up can tell the two apart, which is the point: the
/// device never sees a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Where the pixels are.
    Placed(AtlasTile),
    /// What the pixels are called.
    Named(ResourceKey),
}

impl From<AtlasTile> for Resource {
    fn from(tile: AtlasTile) -> Self {
        Self::Placed(tile)
    }
}

impl From<ResourceKey> for Resource {
    fn from(key: ResourceKey) -> Self {
        Self::Named(key)
    }
}

/// An atlas tile, in the layout an instance carries it in.
///
/// [`AtlasTile`] is the allocator's record and is not laid out for a buffer; this is its encoding
/// for one. Keeping them separate is what lets the atlas stay a pure algorithm with no plain-old-
/// data obligations, while an instance stays copyable as bytes.
///
/// The same six words carry an unresolved [`ResourceKey`] instead, marked by a texture word no pool
/// can produce. A placeholder that reached a device would sample texel zero of texture zero — a
/// plausible-looking wrong glyph rather than a blank one — so it is
/// [checked for](crate::Scene::finish) rather than commented on.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct SpriteTile {
    /// Which texture of which pool, as `kind * 2^16 + index`.
    pub texture: u32,
    /// The allocation's handle within that texture.
    pub tile: u32,
    /// The tile's rectangle in texels, as `[x, y, width, height]`.
    pub bounds: [i32; 4],
}

impl SpriteTile {
    /// The texture word of a sprite whose resource has not been placed.
    ///
    /// Out of the range any pool can produce: the low half is a texture index and the high half is a
    /// pool number, and there are four pools.
    pub const UNRESOLVED: u32 = u32::MAX;

    /// The instance encoding of `tile`.
    pub fn of(tile: AtlasTile) -> Self {
        Self {
            texture: (tile.texture.kind.index() as u32) << 16 | tile.texture.index,
            tile: tile.tile.0,
            bounds: [
                tile.bounds.origin.x,
                tile.bounds.origin.y,
                tile.bounds.size.width,
                tile.bounds.size.height,
            ],
        }
    }

    /// The placeholder a sprite carries while it knows only what it samples.
    ///
    /// The name goes in the words the placement would have occupied, so naming a resource costs an
    /// instance nothing and no side table has to be kept in step with the sprite arrays as they are
    /// sorted.
    pub fn named(key: ResourceKey) -> Self {
        Self {
            texture: Self::UNRESOLVED,
            tile: key.hash() as u32,
            bounds: [
                (key.hash() >> 32) as u32 as i32,
                key.generation().get() as i32,
                key.kind().index() as i32,
                0,
            ],
        }
    }

    /// The encoding of whatever `resource` says.
    pub fn for_resource(resource: Resource) -> Self {
        match resource {
            Resource::Placed(tile) => Self::of(tile),
            Resource::Named(key) => Self::named(key),
        }
    }

    /// Whether this carries a name rather than a placement.
    pub const fn is_unresolved(self) -> bool {
        self.texture == Self::UNRESOLVED
    }

    /// The name this carries, or `None` when it carries a placement instead.
    pub fn key(self) -> Option<ResourceKey> {
        if !self.is_unresolved() {
            return None;
        }
        let hash = u64::from(self.tile) | u64::from(self.bounds[0] as u32) << 32;
        let kind = ResourceKind::ALL.get(self.bounds[2] as usize).copied()?;
        Some(ResourceKey::new(
            kind,
            hash,
            ResourceGeneration::from_raw(self.bounds[1] as u32),
        ))
    }

    /// The sort key a batch of sprites is ordered by, after draw order.
    ///
    /// Texture first, then tile: two sprites at equal draw order are provably non-overlapping, so
    /// their relative order is free, and spending it on clustering by texture is what lets a batch
    /// run until the texture genuinely changes.
    pub fn sort_key(self) -> (u32, u32) {
        (self.texture, self.tile)
    }
}

/// Declares a sprite instance struct with the fields every sprite shares.
macro_rules! sprite {
    ($name:ident, $($doc:literal),+ $(,)?) => {
        $(#[doc = $doc])+
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
        pub struct $name {
            /// Where this draws in the painting order.
            pub order: DrawOrder,
            /// Written zero. Present so the struct has no padding and can be copied as bytes.
            pub reserved: u32,
            /// Where the sprite lands on the surface, as `[x, y, width, height]`.
            pub bounds: [f32; 4],
            /// Premultiplied, gamma-encoded sRGB the coverage is multiplied by.
            pub color: [f32; 4],
            /// The coverage tile.
            pub tile: SpriteTile,
            /// The [`ClipId`] this draws through.
            pub clip: u32,
            /// The slot of the [`SpatialId`] this draws under.
            pub transform: u32,
        }

        impl $name {
            /// A sprite reading `resource` into `bounds`, tinted with `color`.
            pub fn new(
                bounds: Rect<DevicePx, Device>,
                resource: impl Into<Resource>,
                color: Color,
            ) -> Self {
                Self {
                    order: 0,
                    reserved: 0,
                    bounds: [
                        bounds.origin.x.0,
                        bounds.origin.y.0,
                        bounds.size.width.0,
                        bounds.size.height.0,
                    ],
                    color: color.to_premultiplied_srgb(),
                    tile: SpriteTile::for_resource(resource.into()),
                    clip: ClipId::ROOT.0,
                    transform: SpatialId::VIEWPORT.index(),
                }
            }

            /// The same sprite drawn through `clip`.
            pub fn clipped(mut self, clip: ClipId) -> Self {
                self.clip = clip.0;
                self
            }

            /// The rectangle this paints.
            pub fn ink(&self) -> Rect<DevicePx, Device> {
                rect_of(self.bounds)
            }

            /// The clip chain this draws through.
            pub fn clip_id(&self) -> ClipId {
                ClipId(self.clip)
            }
        }
    };
}

sprite!(
    MonoSprite,
    "A single-channel coverage sprite tinted by one colour: an ordinary glyph, or a shape",
    "rasterised as an alpha mask.",
);

sprite!(
    SubpixelSprite,
    "A three-channel coverage sprite, one coverage value per colour channel: LCD-subpixel text.",
    "",
    "Laid out exactly like [`MonoSprite`] and separate only so a batch of one never mixes with a",
    "batch of the other, because they are drawn by different pipelines.",
    "",
    "Emitting one is conditional in two independent ways, and both are the caller's decisions at",
    "emit time. It needs dual-source blending, which not every device has. And it writes no alpha —",
    "the per-channel coverage *is* the blend factor — which is meaningless against a destination",
    "that is not opaque, so a run landing inside an isolated group has to be emitted as a",
    "[`MonoSprite`] instead.",
);

/// A full-colour sprite: an emoji, or a decoded image.
///
/// Its texels are **premultiplied**, like everything else that composites here, so a half-covered
/// edge texel contributes half its colour rather than all of it. Straight-alpha bytes are converted
/// once, when the image is decoded, and never here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ColorSprite {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// One in the low bit when the sprite is drawn desaturated.
    pub flags: u32,
    /// Where the sprite lands on the surface, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// The rectangle the sprite is confined to and the radii are measured against.
    ///
    /// Equal to `bounds` for everything except fitted replaced content. `object-fit` places the
    /// picture at `bounds` — larger than the box for `cover`, smaller for `contain` — while the
    /// box's own rectangle stays here, so the overflow is cut and the rounded corners follow the
    /// box rather than the picture.
    pub frame: [f32; 4],
    /// Elliptical corner radii clipping the sprite, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The colour tile.
    pub tile: SpriteTile,
    /// A multiplier on the sprite's own alpha.
    pub opacity: f32,
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
}

impl ColorSprite {
    /// The flag bit that draws the sprite desaturated.
    pub const GRAYSCALE: u32 = 1;

    /// A sprite reading `resource` into `bounds`, fully opaque and square-cornered.
    pub fn new(bounds: Rect<DevicePx, Device>, resource: impl Into<Resource>) -> Self {
        let bounds = [
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0,
            bounds.size.height.0,
        ];
        Self {
            order: 0,
            flags: 0,
            bounds,
            frame: bounds,
            radii: [0.0; 8],
            tile: SpriteTile::for_resource(resource.into()),
            opacity: 1.0,
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
        }
    }

    /// The same sprite drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip.0;
        self
    }

    /// The same sprite confined to `frame`, which is what fitted replaced content is drawn with.
    pub fn framed(mut self, frame: Rect<DevicePx, Device>) -> Self {
        self.frame = [
            frame.origin.x.0,
            frame.origin.y.0,
            frame.size.width.0,
            frame.size.height.0,
        ];
        self
    }

    /// The same sprite with rounded corners.
    pub fn with_radii(mut self, radii: Corners<Vec2<DevicePx>>) -> Self {
        self.radii = [
            radii.top_left.x.0,
            radii.top_left.y.0,
            radii.top_right.x.0,
            radii.top_right.y.0,
            radii.bottom_right.x.0,
            radii.bottom_right.y.0,
            radii.bottom_left.x.0,
            radii.bottom_left.y.0,
        ];
        self
    }

    /// The rectangle this paints.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        // What is drawn is the picture cut to its frame, so the ink is their intersection: a
        // `cover` picture paints no further than its box, and a letterboxed one no further than
        // itself.
        rect_of(self.bounds)
            .intersection(rect_of(self.frame))
            .unwrap_or_else(|| {
                Rect::new(
                    rect_of(self.bounds).origin,
                    Size::new(DevicePx(0.0), DevicePx(0.0)),
                )
            })
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }
}
