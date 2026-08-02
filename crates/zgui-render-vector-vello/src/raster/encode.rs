//! Turning one planned pass into one scene the path renderer can execute.

use kurbo::Affine;
use peniko::Fill;
use vello::Scene;
use zgui_geom::{Device, Matrix4, Rect};
use zgui_render::{VectorFrame, VectorPass};
use zgui_scene::{PaintTable, VectorItem};

use crate::raster::cached::Encodings;
use crate::raster::paint;

/// What encoding one pass cost, and what it could not do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Encoded {
    /// Clip layers pushed to absorb item residuals.
    ///
    /// Absorbing a clip *moves* the cost of a distinctly clipped item out of the pass count rather
    /// than deleting it: a row of twelve differently clipped avatars is one pass and twelve layers,
    /// not twelve passes. Counting it beside the pass count is what stops that trade reading as a
    /// free win.
    pub clip_layers: u32,
    /// Items dropped because a residual clip had no shape to apply.
    ///
    /// What survives of a sampled mask in a display list is a coverage tile, not the shape it was
    /// rasterised from, and there is nothing here that can sample one. Such an item is left out
    /// rather than drawn under-clipped, because missing content is a thing somebody notices and a
    /// clip that silently did not apply is not.
    pub unclippable: u32,
    /// Items dropped because nothing here paints them, which is a sampled image.
    pub unpaintable: u32,
    /// Items whose transform is not two-dimensional, drawn without it.
    pub flattened_transforms: u32,
}

/// Adds one pass to `scene`, at the surface's own coordinates.
///
/// The scene is not reset, because a scratch layer holds every pass assigned to it and those passes
/// are rasterised together — one scene, one rasterisation, whatever the layer's share of the frame
/// turned out to be. Device coordinates are what makes that sound: two passes on one layer do not
/// overlap on the surface, so they do not overlap in the scene either.
pub fn pass(
    scene: &mut Scene,
    cache: &mut Encodings,
    frame: &VectorFrame<'_>,
    pass: &VectorPass,
) -> Encoded {
    let mut encoded = Encoded::default();
    let shift = Affine::IDENTITY;

    for planned in frame.plan.items_of(pass) {
        let Some(item) = frame.items.get(planned.item) else {
            continue;
        };
        let residual = frame.clips.links(planned.residual);
        let shapes: Option<Vec<(kurbo::BezPath, Affine)>> = residual
            .iter()
            .map(|link| {
                zgui_scene::clip::path::of(link).map(|shape| (shape, space_of(link, frame)))
            })
            .collect::<Option<Vec<_>>>();
        let Some(shapes) = shapes else {
            encoded.unclippable += 1;
            continue;
        };
        let transform = match transform_of(item, frame) {
            Ok(affine) => affine,
            Err(affine) => {
                encoded.flattened_transforms += 1;
                affine
            }
        };
        // The residual is applied outside the item's own transform, because it is the part of the
        // item's *clip chain* the pass's own clip does not cover. Each link's shape is measured in
        // the link's own coordinate system — a clipping box inside a transformed subtree, or one a
        // dialog's held placement is still moving — so each is pushed through that system's matrix,
        // the same matrices the composite's own clip test resolves through. A link outside any
        // transform resolves to the identity and lands where it was recorded.
        for (shape, placement) in &shapes {
            scene.push_clip_layer(Fill::NonZero, shift * *placement, shape);
            encoded.clip_layers += 1;
        }
        // The item's own shape clips are in the item's own space, so they go *inside* the item's
        // transform: a clipped drawing that is rotated has its clip rotated with it.
        let placement = shift * transform;
        for clip in &item.clips {
            scene.push_clip_layer(clip.rule, placement, clip.path.as_ref());
            encoded.clip_layers += 1;
        }
        let painted = encode_item(scene, cache, item, frame.paints, placement);
        if !painted {
            encoded.unpaintable += 1;
        }
        for _ in &item.clips {
            scene.pop_layer();
        }
        for _ in &shapes {
            scene.pop_layer();
        }
    }
    encoded
}

/// The matrix a clip link's own coordinate system resolves to, flattened if it leaves the plane.
///
/// The identity for a link measured in device pixels, which is every clip outside a transform. What
/// a three-dimensional matrix costs here is the transform and never the clip: the same flattening
/// [`transform_of`] applies to the content being clipped.
fn space_of(link: &zgui_scene::ClipLink, frame: &VectorFrame<'_>) -> Affine {
    let space = match link {
        zgui_scene::ClipLink::RoundedRect { space, .. } => *space,
        zgui_scene::ClipLink::Mask { transform, .. } => *transform,
    };
    let Some(matrix) = frame.placements.get(space) else {
        return Affine::IDENTITY;
    };
    if !matrix.is_2d() {
        return Affine::IDENTITY;
    }
    affine_of(matrix)
}

/// Places one item's cached encoding, and says whether anything was painted.
fn encode_item(
    scene: &mut Scene,
    cache: &mut Encodings,
    item: &VectorItem,
    paints: &PaintTable,
    placement: Affine,
) -> bool {
    let fill = item.fill.and_then(|reference| paint::of(reference, paints));
    let stroke = item.stroke.as_ref().and_then(|stroke| {
        paint::of(stroke.paint, paints).map(|brush| (brush, stroke.style.clone()))
    });
    if fill.is_none() && stroke.is_none() {
        return false;
    }
    // Encoded once at the item's own coordinates and re-placed every frame with a copy, rather than
    // re-flattened: curves are flattened on the device, so what is cached here is the encoding and
    // not a polyline.
    let encoding = cache.get(item, paints, |into| {
        if let Some(painted) = &fill {
            into.fill(
                item.fill_rule,
                Affine::IDENTITY,
                &painted.brush,
                painted.transform,
                item.path.as_ref(),
            );
        }
        if let Some((painted, style)) = &stroke {
            into.stroke(
                style,
                Affine::IDENTITY,
                &painted.brush,
                painted.transform,
                item.path.as_ref(),
            );
        }
    });
    scene.append(encoding, Some(placement));
    true
}

/// The item's own transform, or the identity when it is not one this can apply.
///
/// The error arm carries the identity rather than nothing, because a three-dimensional transform is
/// something a scene can legitimately carry and dropping the item over it would delete content; what
/// it costs is the transform, and the count is how anybody finds out.
fn transform_of(item: &VectorItem, frame: &VectorFrame<'_>) -> Result<Affine, Affine> {
    let Some(id) = item.transform else {
        return Ok(Affine::IDENTITY);
    };
    let Some(matrix) = frame.placements.get(id) else {
        return Ok(Affine::IDENTITY);
    };
    if !matrix.is_2d() {
        return Err(Affine::IDENTITY);
    }
    Ok(affine_of(matrix))
}

/// The two-dimensional affine a matrix embeds.
fn affine_of(matrix: &Matrix4) -> Affine {
    let column = matrix.columns;
    Affine::new([
        f64::from(column[0][0]),
        f64::from(column[0][1]),
        f64::from(column[1][0]),
        f64::from(column[1][1]),
        f64::from(column[3][0]),
        f64::from(column[3][1]),
    ])
}

/// The region a pass covers, as the rasterisation's own extent.
pub fn extent(region: Rect<i32, Device>) -> (u32, u32) {
    (
        region.size.width.max(0) as u32,
        region.size.height.max(0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Affine2, Matrix4};

    use super::affine_of;

    #[test]
    fn a_two_dimensional_matrix_keeps_every_coefficient_through_the_embedding() {
        let affine = Affine2::new(2.0, 0.5, -0.25, 3.0, 40.0, -8.0);
        let embedded: Matrix4 = affine.to_matrix4();
        let round_tripped = affine_of(&embedded);
        assert_eq!(
            round_tripped.as_coeffs(),
            [2.0, 0.5, -0.25, 3.0, 40.0, -8.0]
        );
    }
}
