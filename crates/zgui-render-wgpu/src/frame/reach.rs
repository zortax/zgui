//! The device pixels an instanced batch can write.

use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect};
use zgui_scene::{Batch, PrimitiveKind, Scene, SpatialId};

/// One batch of the frame, and the device pixels it can write.
///
/// `reach` is `None` for a batch that is never culled: every batch that is not instanced, and an
/// instanced batch whose extent cannot be bounded on the device.
#[derive(Clone, Debug, PartialEq)]
pub struct Reached {
    /// The batch.
    pub batch: Batch,
    /// The device pixels it can write, or `None` when it is never culled.
    pub reach: Option<Rect<i32, Device>>,
}

/// The batches of `scene` in draw order, each with the device pixels it can write when `bounded`.
///
/// The merge runs once per frame. A frame of several damage rectangles replays this list once per
/// rectangle, and a batch whose reach misses a rectangle draws nothing there.
pub fn batches(scene: &Scene, bounded: bool) -> Vec<Reached> {
    let mut spaces = Spaces::new(scene);
    scene
        .batches()
        .map(|batch| {
            let reach = if bounded {
                reach_of(scene, &mut spaces, &batch)
            } else {
                None
            };
            Reached { batch, reach }
        })
        .collect()
}

/// The device pixels `batch` can write, or `None` when it is never culled.
///
/// The vertex stage draws a quad, shaded quad, shadow or decoration over its bounds grown by one
/// pixel in its own space, and a sprite over its bounds. This is the larger of the two, put through
/// the instance's matrix the same way, and grown by one more pixel for rounding.
fn reach_of(scene: &Scene, spaces: &mut Spaces<'_>, batch: &Batch) -> Option<Rect<i32, Device>> {
    let (kind, range) = match batch {
        Batch::Quads(range) => (PrimitiveKind::Quad, range),
        Batch::Shaded { range, .. } => (PrimitiveKind::Shaded, range),
        Batch::Shadows(range) => (PrimitiveKind::Shadow, range),
        Batch::Decorations(range) => (PrimitiveKind::Decoration, range),
        Batch::MonoSprites { range, .. } => (PrimitiveKind::MonoSprite, range),
        Batch::SubpixelSprites { range, .. } => (PrimitiveKind::SubpixelSprite, range),
        Batch::ColorSprites { range, .. } => (PrimitiveKind::ColorSprite, range),
        // Each of these changes the target, reads a target or draws out of a payload of its own,
        // so it is planned whatever the scissor is.
        Batch::Marks(_)
        | Batch::Vector(_)
        | Batch::External(_)
        | Batch::Backdrop(_)
        | Batch::Group(_) => return None,
    };
    let remap = scene.remap(kind);
    let mut low = (f32::INFINITY, f32::INFINITY);
    let mut high = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &slot in remap.get(range.clone())? {
        let (bounds, space) = geometry(scene, kind, slot as usize)?;
        let local = Rect::from_corners(
            Point::new(DevicePx(bounds[0] - 1.0), DevicePx(bounds[1] - 1.0)),
            Point::new(
                DevicePx(bounds[0] + bounds[2] + 1.0),
                DevicePx(bounds[1] + bounds[3] + 1.0),
            ),
        );
        let placed = spaces.on_device(space, local)?;
        low = (low.0.min(placed.left().0), low.1.min(placed.top().0));
        high = (high.0.max(placed.right().0), high.1.max(placed.bottom().0));
    }
    if !(low.0.is_finite() && low.1.is_finite() && high.0.is_finite() && high.1.is_finite()) {
        return None;
    }
    Some(Rect::from_corners(
        Point::new(low.0.floor() as i32 - 1, low.1.floor() as i32 - 1),
        Point::new(high.0.ceil() as i32 + 1, high.1.ceil() as i32 + 1),
    ))
}

/// The bounds field and the coordinate-system slot of the primitive at `slot` of `kind`'s array.
fn geometry(scene: &Scene, kind: PrimitiveKind, slot: usize) -> Option<([f32; 4], u32)> {
    let primitives = &scene.primitives;
    Some(match kind {
        PrimitiveKind::Quad => primitives
            .quads
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::Shaded => primitives
            .shaded
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::Shadow => primitives
            .shadows
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::Decoration => primitives
            .decorations
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::MonoSprite => primitives
            .mono_sprites
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::SubpixelSprite => primitives
            .subpixel_sprites
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        PrimitiveKind::ColorSprite => primitives
            .color_sprites
            .get(slot)
            .map(|p| (p.bounds, p.transform))?,
        _ => return None,
    })
}

/// The scene's coordinate systems, resolved once per slot in a row.
///
/// The primitives of one batch mostly share a coordinate system, so the last slot and its matrix
/// are kept.
struct Spaces<'scene> {
    /// The scene whose coordinate systems these are.
    scene: &'scene Scene,
    /// The last slot resolved, and its matrix when it maps onto the device by an affine map.
    last: Option<(u32, Option<Matrix4>)>,
}

impl<'scene> Spaces<'scene> {
    /// No slot resolved yet.
    fn new(scene: &'scene Scene) -> Self {
        Self { scene, last: None }
    }

    /// `rect`, measured in the coordinate system at `slot`, on the device.
    ///
    /// `None` when the system is not bounded on the device: a slot nothing occupies, or a matrix
    /// with a perspective part. Under perspective a corner can be behind the eye, and the bounds of
    /// the corners in front of it are then too small.
    fn on_device(
        &mut self,
        slot: u32,
        rect: Rect<DevicePx, Device>,
    ) -> Option<Rect<DevicePx, Device>> {
        if slot == SpatialId::VIEWPORT.index() {
            return Some(rect);
        }
        let matrix = match self.last {
            Some((held, matrix)) if held == slot => matrix,
            _ => {
                let matrix = self.scene.spatial.resolve_at(slot).filter(maps_affinely);
                self.last = Some((slot, matrix));
                matrix
            }
        };
        Some(zgui_geom::transformed_bounds(&matrix?, rect))
    }
}

/// Whether `matrix` maps the z = 0 plane onto the device with one positive `w` everywhere.
fn maps_affinely(matrix: &Matrix4) -> bool {
    let m = &matrix.columns;
    m[0][3] == 0.0 && m[1][3] == 0.0 && m[3][3] > 0.0
}

#[cfg(test)]
mod tests {
    use super::{Reached, batches};
    use zgui_bits::DamageSet;
    use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
    use zgui_scene::{Batch, OwnSpace, Paint, PropertyOwner, Quad, Scene};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// A finished scene of one quad at `at`, drawn under `matrix` when one is given.
    fn one_quad(at: Rect<DevicePx, Device>, matrix: Option<Matrix4>) -> Scene {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(256, 256));
        let paint = scene.paints.add(Paint::Solid(zgui_color::Color::BLACK));
        let mut quad = Quad::filled(at, paint);
        if let Some(matrix) = matrix {
            let owner = PropertyOwner::new(7).expect("a handle is never the empty word");
            let viewport = scene.spatial.viewport();
            let space =
                scene
                    .spatial
                    .space_of(viewport, owner, OwnSpace::of(Some(matrix), None, false));
            quad = quad.transformed(space);
        }
        scene.push_quad(quad);
        scene.finish(&DamageSet::full());
        scene
    }

    #[test]
    fn a_quad_reaches_its_bounds_and_the_pixels_its_edge_smooths_into() {
        let scene = one_quad(rect(10.5, 20.0, 30.0, 8.0), None);
        let reached = batches(&scene, true);
        assert_eq!(
            reached,
            vec![Reached {
                batch: Batch::Quads(0..1),
                reach: Some(Rect::from_corners(Point::new(8, 18), Point::new(43, 30))),
            }]
        );
    }

    #[test]
    fn an_unbounded_list_reaches_nowhere_in_particular() {
        let scene = one_quad(rect(10.0, 20.0, 30.0, 8.0), None);
        assert_eq!(batches(&scene, false)[0].reach, None);
    }

    #[test]
    fn a_moved_coordinate_system_moves_the_reach() {
        let scene = one_quad(
            rect(0.0, 0.0, 10.0, 10.0),
            Some(Matrix4::translation(100.0, 50.0, 0.0)),
        );
        assert_eq!(
            batches(&scene, true)[0].reach,
            Some(Rect::from_corners(Point::new(98, 48), Point::new(112, 62)))
        );
    }

    #[test]
    fn a_tilt_under_perspective_is_never_culled() {
        // `w` grows across the plane, so the projected corners do not bound what is drawn.
        let tilted = Matrix4::from_columns([
            [1.0, 0.0, 0.0, 0.001],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let scene = one_quad(rect(0.0, 0.0, 10.0, 10.0), Some(tilted));
        assert_eq!(batches(&scene, true)[0].reach, None);
    }
}
