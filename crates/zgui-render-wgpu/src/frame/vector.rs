//! The half of the rasteriser contract that only a graphics API can state.

use zgui_geom::Rect;
use zgui_render::{VectorPass, VectorPlan, VectorRaster, VectorTarget};

use crate::pipeline::vector::VectorInstance;

/// A vector rasteriser this renderer can composite the results of.
///
/// The backend-neutral contract deliberately says nothing about what a rasteriser wrote into: one
/// implementation writes layers of an array texture, another resolves multisampled scratch, and
/// naming either shape in a crate that must not name a graphics API would make the other one wrong.
/// This is the one extra question a renderer has to ask, and it is asked here rather than by
/// downcasting, so a second implementation is a second implementation of a trait rather than a
/// special case in the renderer.
///
/// # What the answer has to hold
///
/// Straight — un-premultiplied — colour, in an unencoded eight-bit format, with one texel per
/// device pixel, **at the surface's own coordinates**: the pass's [`region`](VectorPass::region)
/// covers the texels of that region and no others. The composite premultiplies as it reads.
///
/// Device coordinates rather than a corner of their own is what lets two passes that do not meet on
/// the screen share one scratch, and it is the whole reason the scratch is a function of the surface
/// rather than of the number of passes the busiest frame of the session planned. Anything a pass did
/// not paint has to read as fully transparent, which is what the mandatory pre-clear is for: a texel
/// left over from the previous frame composites as *wrong* pixels rather than missing ones.
pub trait VectorSource: VectorRaster {
    /// A view of whatever the rasteriser put a pass's result into.
    ///
    /// `None` for a target this implementation does not recognise, which costs the composite rather
    /// than drawing it against a stranger's texture.
    fn view(&self, target: VectorTarget) -> Option<&wgpu::TextureView>;
}

/// The composite quads one planned pass is drawn with.
///
/// A pass is composited either as a single quad over its whole region, binding the one clip every
/// item of it genuinely has, or as one quad per item, each reading only that item's part of the
/// scratch and binding that item's own clip. **Which of the two is not decided here**: the display
/// list sets the flag, and it sets it exactly when no two items of the pass overlap each other — so
/// no part of the scratch is ever composited twice. A backend that chose for itself would
/// double-blend every overlap.
pub fn instances_of(plan: &VectorPlan, pass: &VectorPass) -> Vec<VectorInstance> {
    let origin = pass.region.origin;
    // The scratch is in device coordinates, so every quad reads the texels it covers and the source
    // is the destination. A quad reading a corner of its own would be reading whichever *other*
    // pass shares the layer, because sharing is what a device-space scratch buys.
    if !pass.instanced {
        return vec![VectorInstance::new(
            pass.region,
            (origin.x, origin.y),
            pass.clip.0,
        )];
    }
    plan.items_of(pass)
        .iter()
        .map(|item| {
            let bounds = Rect::new(
                zgui_geom::Point::new(origin.x + item.ink.origin.x, origin.y + item.ink.origin.y),
                item.ink.size,
            );
            VectorInstance::new(bounds, (bounds.origin.x, bounds.origin.y), item.clip.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Point, Rect, Size};
    use zgui_render::{VectorPass, VectorPlan, VectorTarget};
    use zgui_scene::{ClipId, PlannedItem};

    use super::instances_of;

    fn item(item: usize, ink: Rect<i32, zgui_geom::Device>, clip: u32) -> PlannedItem {
        PlannedItem {
            item,
            residual: ClipId::ROOT,
            clip: ClipId(clip),
            ink,
        }
    }

    fn plan(instanced: bool) -> (VectorPlan, VectorPass) {
        let plan = VectorPlan {
            passes: Vec::new(),
            items: vec![
                item(0, Rect::new(Point::new(0, 0), Size::new(16, 16)), 3),
                item(1, Rect::new(Point::new(32, 0), Size::new(16, 16)), 5),
            ],
        };
        let pass = VectorPass {
            region: Rect::new(Point::new(64, 128), Size::new(64, 32)),
            target: VectorTarget(0),
            items: 0..2,
            clip: ClipId(1),
            instanced,
        };
        (plan, pass)
    }

    #[test]
    fn a_whole_pass_composite_is_one_quad_over_the_region_binding_the_shared_clip() {
        let (plan, pass) = plan(false);
        let instances = instances_of(&plan, &pass);
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].bounds, [64.0, 128.0, 64.0, 32.0]);
        assert_eq!(
            [instances[0].source[0], instances[0].source[1]],
            [64.0, 128.0],
            "a device-space scratch is read where it is written, or the quad reads whichever other \
             pass shares the layer"
        );
        assert_eq!(instances[0].control[0], 1.0);
    }

    #[test]
    fn an_instanced_composite_is_one_quad_per_item_each_with_its_own_clip() {
        let (plan, pass) = plan(true);
        let instances = instances_of(&plan, &pass);
        assert_eq!(instances.len(), 2);
        // Each quad lands at the region's origin plus the item's own offset within it, and reads
        // exactly those texels out of the scratch — which is what makes the union of the quads read
        // every painted texel once and no texel twice.
        assert_eq!(instances[0].bounds, [64.0, 128.0, 16.0, 16.0]);
        assert_eq!(instances[1].bounds, [96.0, 128.0, 16.0, 16.0]);
        assert_eq!(
            [instances[1].source[0], instances[1].source[1]],
            [96.0, 128.0]
        );
        assert_eq!(instances[0].control[0], 3.0);
        assert_eq!(instances[1].control[0], 5.0);
    }
}
