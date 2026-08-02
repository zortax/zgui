//! The side tables, in the form a shader reads them.
//!
//! Clips, paints and transforms are addressed by index from every instance, so they travel in
//! storage buffers rather than in the instances themselves: an N-stop gradient then costs a quad
//! exactly as many instance bytes as a flat colour.

use bytemuck::{Pod, Zeroable};
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation, Interpolation};
use zgui_geom::Matrix4;
use zgui_scene::{ClipTable, GradientKind, Paint, PaintTable, ResolvedClip, Scene, SpriteTile};

/// One rounded-corner test of a clip chain.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuRounded {
    /// The rectangle, as `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// Elliptical radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
}

/// A whole clip chain, flattened into what one draw call applies.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuClip {
    /// The intersection of every rectangle in the chain.
    pub aabb: [f32; 4],
    /// The outermost rounded test.
    pub first: GpuRounded,
    /// The innermost rounded test.
    pub second: GpuRounded,
    /// How many of the two are meaningful.
    pub count: u32,
    /// One when the chain samples a coverage mask.
    pub has_mask: u32,
    /// The mask tile, meaningless unless `has_mask` is set.
    pub mask: SpriteTile,
}

/// One paint source.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuPaint {
    /// The family: nothing, solid, gradient or image.
    pub kind: u32,
    /// The gradient's shape: linear, radial or conic.
    pub gradient: u32,
    /// The space the ramp's stops are written in.
    pub space: u32,
    /// One when the ramp repeats outside its extent.
    pub flags: u32,
    /// The ramp's geometry, meaning whatever its shape says it does.
    pub geometry: [f32; 4],
    /// The colour of a solid paint, premultiplied and gamma-encoded.
    pub color: [f32; 4],
    /// Where this paint's stops begin.
    pub stop_start: u32,
    /// How many stops it has.
    pub stop_count: u32,
    /// Written zero, so the structure has no padding.
    pub pad0: u32,
    /// Written zero, so the structure has no padding.
    pub pad1: u32,
}

/// One stop of a ramp, in the space the ramp is interpolated in, premultiplied.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuStop {
    /// The three channels of the interpolation space, then the alpha.
    pub color: [f32; 4],
    /// Where along the ramp this stop sits.
    pub offset: f32,
    /// Written zero, so the structure has no padding.
    pub pad: [f32; 3],
}

/// One coordinate system: the matrix mapping it onto the device, column by column.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GpuSpatial {
    /// The matrix, as its four columns.
    pub matrix: [[f32; 4]; 4],
}

impl Default for GpuSpatial {
    fn default() -> Self {
        Self::of(Matrix4::IDENTITY)
    }
}

impl GpuSpatial {
    /// The shader's form of `matrix`.
    pub fn of(matrix: Matrix4) -> Self {
        Self {
            matrix: matrix.columns,
        }
    }
}

/// The family discriminants, which travel in the instance so a solid fill never reads the table.
pub mod kind {
    /// Paints nothing.
    pub const NONE: u32 = 0;
    /// One colour everywhere.
    pub const SOLID: u32 = 1;
    /// A ramp between stops.
    pub const GRADIENT: u32 = 2;
    /// A sampled image.
    pub const IMAGE: u32 = 3;
}

/// The interpolation spaces the shader can convert back from, and what it calls each.
pub mod space {
    /// Gamma-encoded sRGB, which needs no conversion at all.
    pub const SRGB: u32 = 0;
    /// Oklab.
    pub const OKLAB: u32 = 1;
    /// sRGB primaries with a linear transfer function.
    pub const LINEAR_SRGB: u32 = 2;
}

/// Every side table of one frame, flattened.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// The clip chains.
    pub clips: Vec<GpuClip>,
    /// The paint sources.
    pub paints: Vec<GpuPaint>,
    /// Every ramp's stops, concatenated.
    pub stops: Vec<GpuStop>,
    /// Every coordinate system, addressed by the slot a primitive names.
    pub spatial: Vec<GpuSpatial>,
}

impl Tables {
    /// Flattens `scene`'s side tables.
    pub fn of(scene: &Scene) -> Self {
        Self {
            clips: clips(&scene.clips, &scene.spatial),
            spatial: spatial(&scene.spatial),
            ..paints(&scene.paints)
        }
    }
}

/// Every clip chain, resolved into what a draw call binds.
///
/// One entry per slot rather than per live chain, for the reason [`spatial`] states: a primitive
/// names its chain by slot and the shader indexes this array with that number directly, so a count
/// of live chains would both truncate the array and shift every chain above a hole onto the wrong
/// entry.
///
/// Resolved through the frame's matrices, because the shader evaluates a clip at the device pixel
/// and a link interned inside a transformed subtree is measured in that subtree's own space — the
/// same matrices the content it clips is drawn through, read at the same moment.
fn clips(table: &ClipTable, spaces: &zgui_scene::SpatialTree) -> Vec<GpuClip> {
    (0..table.slots())
        .map(|index| {
            let id = zgui_scene::ClipId(index as u32);
            if table.contains(id) {
                gpu_clip(&table.resolve_placed(id, &|space| spaces.resolve(space)))
            } else {
                freed_clip()
            }
        })
        .collect()
}

/// What a slot no chain occupies holds.
///
/// An empty intersection rectangle, so a stale name clips its content away. Resolving the slot
/// instead would answer with the chain that clips *nothing*, because a chain nobody can reach has no
/// links — and a name that has outlived its chain would draw content across the whole surface with
/// nothing reporting it. Of the two ways to be wrong, the one that shows a missing box is the one
/// that can be seen.
fn freed_clip() -> GpuClip {
    GpuClip::default()
}

/// One resolved chain, in the shader's form.
fn gpu_clip(clip: &ResolvedClip) -> GpuClip {
    // A chain that samples a raster mask is applied where the mask is: in a target of its own.
    // Nothing binds one to a direct draw, and if anything ever does, this is where it is noticed
    // rather than where the mask is quietly ignored.
    debug_assert!(
        clip.mask.is_none(),
        "a raster mask cannot be applied by a direct draw"
    );
    GpuClip {
        aabb: clip.aabb,
        first: GpuRounded {
            rect: clip.rounded[0].rect,
            radii: clip.rounded[0].radii,
        },
        second: GpuRounded {
            rect: clip.rounded[1].rect,
            radii: clip.rounded[1].radii,
        },
        count: clip.rounded_count,
        has_mask: u32::from(clip.mask.is_some()),
        mask: clip.mask.map(SpriteTile::of).unwrap_or_default(),
    }
}

/// Every coordinate system, resolved, in the shader's form.
///
/// One entry per slot rather than per live node, because a primitive names its coordinate system by
/// slot and the shader indexes this array with that number directly. A slot nothing occupies holds
/// the identity, so a stale name reads as content drawn where it was laid out rather than as
/// whatever the slot's bytes happened to be.
fn spatial(tree: &zgui_scene::SpatialTree) -> Vec<GpuSpatial> {
    zgui_scene::Placements::of(tree)
        .matrices()
        .map(GpuSpatial::of)
        .collect()
}

/// Every paint source and its stops.
///
/// One entry per slot, for the same reason [`clips`] is: an instance carries the slot number, and a
/// slot nothing occupies paints nothing.
fn paints(table: &PaintTable) -> Tables {
    let mut tables = Tables::default();
    for index in 0..table.slots() {
        let id = zgui_scene::PaintId(index as u32);
        let Some(paint) = table.get(id) else {
            tables.paints.push(GpuPaint::default());
            continue;
        };
        tables.paints.push(match paint {
            Paint::Solid(color) => GpuPaint {
                kind: kind::SOLID,
                color: color.to_premultiplied_srgb(),
                ..GpuPaint::default()
            },
            Paint::Gradient {
                kind: shape,
                stops,
                space: interpolation_space,
                hue,
                repeating,
            } => {
                let start = tables.stops.len() as u32;
                let (encoding, written) = ramp(stops, *interpolation_space, *hue);
                tables.stops.extend(written);
                GpuPaint {
                    kind: kind::GRADIENT,
                    gradient: gradient_tag(shape),
                    space: encoding,
                    flags: u32::from(*repeating),
                    geometry: gradient_geometry(shape),
                    color: [0.0; 4],
                    stop_start: start,
                    stop_count: tables.stops.len() as u32 - start,
                    pad0: 0,
                    pad1: 0,
                }
            }
            Paint::Image { .. } => GpuPaint {
                kind: kind::IMAGE,
                ..GpuPaint::default()
            },
        });
    }
    tables
}

/// Which shape a ramp follows.
fn gradient_tag(kind: &GradientKind) -> u32 {
    match kind {
        GradientKind::Linear { .. } => 0,
        GradientKind::Radial { .. } => 1,
        GradientKind::Conic { .. } => 2,
    }
}

/// A ramp's geometry, packed the way its shape reads it.
fn gradient_geometry(kind: &GradientKind) -> [f32; 4] {
    match kind {
        GradientKind::Linear { start, end } => [start.x.0, start.y.0, end.x.0, end.y.0],
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => [center.x.0, center.y.0, *radius_x, *radius_y],
        GradientKind::Conic { center, from_angle } => [center.x.0, center.y.0, 0.0, *from_angle],
    }
}

/// A ramp's stops, and which space the shader must read them in.
///
/// Three spaces are walked in the shader, because each is a straight line in coordinates it can
/// convert back from with a handful of arithmetic. Every other space — the polar ones, the wide
/// gamuts, CIE Lab — is approximated here instead, by adding stops along the true curve until the
/// straight lines between them are within an eight-bit step of it. Two mechanisms, one rule: the
/// ramp is walked where walking it is exact, and approximated where it is not.
fn ramp(stops: &[GradientStop], space: ColorSpace, hue: HueInterpolation) -> (u32, Vec<GpuStop>) {
    let interpolation = Interpolation::new(space).with_hue(hue);
    match space {
        ColorSpace::Srgb => (space::SRGB, stops.iter().map(premultiplied_srgb).collect()),
        ColorSpace::Oklab => (
            space::OKLAB,
            stops
                .iter()
                .map(|stop| premultiplied_components(stop, ColorSpace::Oklab))
                .collect(),
        ),
        ColorSpace::SrgbLinear => (
            space::LINEAR_SRGB,
            stops
                .iter()
                .map(|stop| premultiplied_components(stop, ColorSpace::SrgbLinear))
                .collect(),
        ),
        _ => (
            space::SRGB,
            zgui_color::densify(stops, interpolation)
                .iter()
                .map(premultiplied_srgb)
                .collect(),
        ),
    }
}

/// One stop as premultiplied gamma-encoded sRGB.
fn premultiplied_srgb(stop: &GradientStop) -> GpuStop {
    GpuStop {
        color: stop.color.to_premultiplied_srgb(),
        offset: stop.offset,
        pad: [0.0; 3],
    }
}

/// One stop as premultiplied components of `space`.
///
/// The premultiplication is what CSS specifies for gradient interpolation, and it is why a ramp
/// running to `transparent` does not darken through its middle.
fn premultiplied_components(stop: &GradientStop, space: ColorSpace) -> GpuStop {
    let converted: Color = stop.color.to_space(space);
    let alpha = converted.alpha();
    let [first, second, third] = converted.components();
    GpuStop {
        color: [first * alpha, second * alpha, third * alpha, alpha],
        offset: stop.offset,
        pad: [0.0; 3],
    }
}

/// What a table with a freed slot uploads.
#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::{ClipId, ClipLink, ClipTable, PaintTable, ResolvedClip};

    use super::{clips, freed_clip, gpu_clip, kind, paints};

    /// A device rectangle.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// A clip table of three slots whose middle one has been freed, with the id of the survivor
    /// above the hole.
    fn clips_with_a_hole() -> (ClipTable, ClipId, ClipId) {
        let mut table = ClipTable::rooted();
        table.begin_frame();
        let doomed = table.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
        table.begin_frame();
        let kept = table.only(ClipLink::rect(rect(4.0, 4.0, 2.0, 2.0)));
        table.begin_frame();
        assert_eq!(
            table.evict_least_recently_used(),
            1,
            "only the coldest goes"
        );
        assert!(
            !table.contains(doomed),
            "the hole is where the test wants it"
        );
        (table, doomed, kept)
    }

    #[test]
    fn a_table_with_a_hole_uploads_every_slot() {
        let (table, doomed, _) = clips_with_a_hole();
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_eq!(uploaded.len(), table.slots(), "one entry per slot");
        assert_eq!(uploaded.len(), table.len() + 1, "and one slot is a hole");
        assert_eq!(uploaded[doomed.0 as usize], freed_clip());

        let mut paint_table = PaintTable::new();
        paint_table.begin_frame();
        let gone = paint_table.solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
        paint_table.begin_frame();
        paint_table.solid(zgui_color::Color::srgb(0.0, 1.0, 0.0, 1.0));
        paint_table.begin_frame();
        assert_eq!(paint_table.evict_least_recently_used(), 1);
        let painted = paints(&paint_table).paints;
        assert_eq!(painted.len(), paint_table.slots());
        assert_eq!(
            painted[gone.0 as usize].kind,
            kind::NONE,
            "a hole paints nothing"
        );
    }

    #[test]
    fn a_freed_clip_slot_does_not_read_as_unbounded() {
        let (table, doomed, _) = clips_with_a_hole();
        assert_eq!(
            table.resolve(doomed),
            ResolvedClip::unbounded(),
            "resolving a freed slot is the trap this guards"
        );
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_ne!(
            uploaded[doomed.0 as usize],
            gpu_clip(&ResolvedClip::unbounded()),
            "a stale name must not draw across the whole surface"
        );
        assert_eq!(uploaded[doomed.0 as usize].aabb, [0.0; 4], "it clips away");
    }

    #[test]
    fn a_primitive_naming_a_slot_above_a_hole_resolves_to_its_own_entry() {
        let (table, doomed, kept) = clips_with_a_hole();
        assert!(kept.0 > doomed.0, "the survivor is above the hole");
        let uploaded = clips(&table, &zgui_scene::SpatialTree::with_viewport());
        assert_eq!(uploaded[kept.0 as usize], gpu_clip(&table.resolve(kept)));
        assert_eq!(uploaded[kept.0 as usize].aabb, [4.0, 4.0, 2.0, 2.0]);
    }
}
