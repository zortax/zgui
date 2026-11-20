//! Which parts of a frame something replaces outright.

use zgui_geom::{Device, Point, Rect};

use crate::batch::Batch;
use crate::id::{ClipId, DrawOrder};
use crate::paint::{Paint, PaintKind};
use crate::prim::{PrimitiveKind, Quad};
use crate::scene::Scene;
use crate::spatial::SpatialId;

/// How many covering rectangles [`Scene::opaque_covers`] keeps.
///
/// What it is for is the page background and the one or two opaque panels over it. Keeping the
/// largest few bounds the work a caller does per region, and a document with thousands of opaque
/// boxes is not one where the small ones would have answered anything.
const KEPT: usize = 4;

/// A rectangle this frame fills opaquely, and where in the frame it does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpaqueCover {
    /// The pixels its fill certainly replaces.
    pub rect: Rect<i32, Device>,
    /// Its position in the quads' draw-order list, which is where a replay may pick the stream up.
    pub at: usize,
    /// Its draw order, which is what a primitive of another kind is hidden by.
    pub order: DrawOrder,
}

impl Scene {
    /// The largest rectangles this frame fills with an opaque colour, replacing what lies under
    /// them, written into `into`.
    ///
    /// A renderer that redraws part of a frame erases that part first, because what it draws over
    /// it may be translucent and would otherwise blend with the frame before. Where one of these
    /// rectangles covers the part being redrawn, everything up to the fill is hidden by it: the
    /// erasure, and every primitive under it. Both can be left out, which on a damage-limited
    /// frame is a draw call and a pass over everything that changed apiece.
    ///
    /// Only rectangles drawn straight onto the frame's own surface are here: the scan stops at the
    /// first group, whose content goes to a target of its own. Every condition tested is one under
    /// which a fill provably reaches every pixel of its own bounds, and the bounds are given as
    /// whole pixels inset by the one its edge antialiasing softens.
    pub fn opaque_covers(&self, into: &mut Vec<OpaqueCover>) {
        into.clear();
        // A backdrop reads the surface beneath it, and every pixel it reads has to have been
        // written by this frame. Leaving an erasure out is exactly leaving some of them holding
        // the frame before, so a frame with a backdrop in it answers nothing.
        if !self.primitives.backdrops.is_empty() {
            return;
        }
        let remap = self.remap(PrimitiveKind::Quad);
        for batch in self.batches() {
            match batch {
                Batch::Quads(range) => {
                    for at in range {
                        let Some(quad) = remap
                            .get(at)
                            .and_then(|slot| self.primitives.quads.get(*slot as usize))
                        else {
                            continue;
                        };
                        if self.replaces_its_bounds(quad) {
                            keep(
                                into,
                                OpaqueCover {
                                    rect: covered(quad),
                                    at,
                                    order: quad.order,
                                },
                            );
                        }
                    }
                }
                // Everything past the first group is drawn somewhere this cannot answer for.
                Batch::Group(_) => return,
                _ => {}
            }
        }
    }

    /// The part of `batch` that `cover` does not hide, or `None` when it hides all of it.
    ///
    /// Everything drawn before an opaque fill is replaced by it, so a replay that has to produce
    /// only the finished pixels may begin at the fill. Quads are cut exactly, because the fill is
    /// one of them and the list is in draw order; a batch of any other kind is kept whole unless
    /// every primitive in it is hidden, which is all that is worth deciding for a batch the fill
    /// cannot be inside.
    pub fn behind(&self, batch: Batch, cover: &OpaqueCover) -> Option<Batch> {
        let kept = |kind: PrimitiveKind, range: &core::ops::Range<usize>| {
            // The list is in draw order, so the last one is the latest, and a batch whose latest
            // primitive is still under the fill is wholly under it.
            self.order_at(kind, range.end.saturating_sub(1))
                .is_none_or(|order| order >= cover.order)
        };
        match batch {
            Batch::Quads(range) => {
                let start = range.start.max(cover.at);
                (start < range.end).then_some(Batch::Quads(start..range.end))
            }
            Batch::Shadows(ref range) => kept(PrimitiveKind::Shadow, range).then_some(batch),
            Batch::Decorations(ref range) => {
                kept(PrimitiveKind::Decoration, range).then_some(batch)
            }
            Batch::MonoSprites { ref range, .. } => {
                kept(PrimitiveKind::MonoSprite, range).then_some(batch)
            }
            Batch::SubpixelSprites { ref range, .. } => {
                kept(PrimitiveKind::SubpixelSprite, range).then_some(batch)
            }
            Batch::ColorSprites { ref range, .. } => {
                kept(PrimitiveKind::ColorSprite, range).then_some(batch)
            }
            // An application effect draws in order like anything else, and what an opaque fill
            // hides of it is hidden the same way. It can never *be* a cover: an effect may draw
            // anything at all, so nothing about its pixels is known here.
            Batch::Shaded { ref range, .. } => kept(PrimitiveKind::Shaded, range).then_some(batch),
            // Marks are a payload of their own and draw in order like the rest, so what an opaque
            // fill hides of them is decided the same way. A mark can never be a cover: the payload
            // says what to draw and not what it covers.
            Batch::Marks(ref range) => kept(PrimitiveKind::Marks, range).then_some(batch),
            // A group marker is never dropped: the scan that found the cover stopped at the first
            // one, so no marker is under it, and dropping half a pair would leave a target open.
            Batch::Group(_) => Some(batch),
            Batch::Vector(index) => self
                .primitives
                .vectors
                .get(index)
                .is_none_or(|held| held.order >= cover.order)
                .then_some(batch),
            Batch::External(index) => self
                .primitives
                .externals
                .get(index)
                .is_none_or(|held| held.order >= cover.order)
                .then_some(batch),
            // A frame with one in it produces no cover at all, so this is unreachable rather than
            // a decision.
            Batch::Backdrop(_) => Some(batch),
        }
    }

    /// Whether `quad` writes every pixel of its own bounds, whatever was there before.
    ///
    /// Deliberately narrow. A rounded corner leaves the pixels outside the arc alone, a border may
    /// be dashed or translucent, a clip may admit only part of the bounds, a transform may turn the
    /// rectangle to an angle, and a gradient or an image may carry alpha anywhere in it. Each is a
    /// reason the fill might not reach a pixel, and none of them is worth deciding case by case.
    fn replaces_its_bounds(&self, quad: &Quad) -> bool {
        quad.radii == [0.0; 8]
            && quad.border == [0.0; 4]
            && quad.clip == ClipId::ROOT.0
            && quad.transform == SpatialId::VIEWPORT.index()
            && quad.fill.kind == PaintKind::Solid as u32
            && matches!(
                quad.fill.id().and_then(|id| self.paints.get(id)),
                Some(Paint::Solid(colour)) if colour.is_opaque()
            )
    }
}

/// `quad`'s bounds as whole pixels its fill certainly reaches.
///
/// Rounded inwards and then inset by one, because an edge landing inside a pixel gives that pixel
/// partial coverage and a pixel is either wholly replaced or of no use here.
fn covered(quad: &Quad) -> Rect<i32, Device> {
    let [x, y, width, height] = quad.bounds;
    let left = (x.ceil() as i32).saturating_add(1);
    let top = (y.ceil() as i32).saturating_add(1);
    let right = ((x + width).floor() as i32).saturating_sub(1);
    let bottom = ((y + height).floor() as i32).saturating_sub(1);
    Rect::from_corners(
        Point::new(left, top),
        Point::new(right.max(left), bottom.max(top)),
    )
}

/// Adds `cover` to the largest [`KEPT`] held, dropping whatever it makes redundant.
///
/// Covers arrive in draw order, so this one is the latest and hides everything the ones before it
/// hide. One it contains can therefore answer nothing this cannot answer better, and goes. One it
/// sits inside stays: it reaches pixels this does not, and a caller takes whichever of them covers
/// the region it is asking about.
fn keep(held: &mut Vec<OpaqueCover>, cover: OpaqueCover) {
    if cover.rect.is_empty() {
        return;
    }
    held.retain(|other| !cover.rect.contains_rect(other.rect));
    held.push(cover);
    if held.len() > KEPT {
        // The smallest of them covers the least and is the one worth losing.
        let area = |cover: &OpaqueCover| {
            i64::from(cover.rect.size.width.max(0)) * i64::from(cover.rect.size.height.max(0))
        };
        let smallest = (0..held.len())
            .min_by_key(|index| area(&held[*index]))
            .unwrap_or(0);
        held.swap_remove(smallest);
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::DamageSet;
    use zgui_color::Color;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use crate::paint::PaintRef;
    use crate::prim::Quad;
    use crate::scene::Scene;

    /// A scene over a hundred-pixel surface, and an opaque paint to fill with.
    fn surface(alpha: f32) -> (Scene, PaintRef) {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(100, 100));
        let id = scene.paints.solid(Color::srgb(1.0, 1.0, 1.0, alpha));
        (scene, PaintRef::solid(id))
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    fn covers(scene: &mut Scene) -> Vec<Rect<i32, Device>> {
        scene.finish(&DamageSet::full());
        let mut held = Vec::new();
        scene.opaque_covers(&mut held);
        held.iter().map(|cover| cover.rect).collect()
    }

    /// The whole surface, less the pixel each edge softens.
    fn inset() -> Rect<i32, Device> {
        Rect::from_corners(Point::new(1, 1), Point::new(99, 99))
    }

    #[test]
    fn an_opaque_square_cornered_fill_covers_its_own_bounds() {
        let (mut scene, fill) = surface(1.0);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        assert_eq!(covers(&mut scene), vec![inset()]);
    }

    #[test]
    fn a_translucent_fill_covers_nothing() {
        let (mut scene, fill) = surface(0.5);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        assert!(
            covers(&mut scene).is_empty(),
            "a fill that lets the pixel beneath show through replaces nothing"
        );
    }

    #[test]
    fn a_rounded_or_bordered_fill_covers_nothing() {
        for touch in [
            |quad: &mut Quad| quad.radii[0] = 4.0,
            |quad: &mut Quad| quad.border[2] = 1.0,
        ] {
            let (mut scene, fill) = surface(1.0);
            let mut quad = Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill);
            touch(&mut quad);
            scene.push_quad(quad);
            assert!(
                covers(&mut scene).is_empty(),
                "a corner the fill curves away from, or an edge a border owns, is a pixel it does \
                 not reach"
            );
        }
    }

    #[test]
    fn a_later_fill_over_the_same_pixels_replaces_the_one_before_it() {
        let (mut scene, fill) = surface(1.0);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        scene.finish(&DamageSet::full());
        let mut held = Vec::new();
        scene.opaque_covers(&mut held);
        assert_eq!(
            held.len(),
            1,
            "one covers the other, so one of them is kept"
        );
        assert_eq!(
            held[0].at, 1,
            "and it is the later one, because a replay may start at it and skip more"
        );
    }

    #[test]
    fn a_fill_inside_another_is_kept_beside_it() {
        let (mut scene, fill) = surface(1.0);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        scene.push_quad(Quad::filled(rect(20.0, 20.0, 40.0, 40.0), fill));
        let held = covers(&mut scene);
        assert_eq!(
            held,
            vec![
                inset(),
                Rect::from_corners(Point::new(21, 21), Point::new(59, 59))
            ],
            "the inner one hides more and the outer one reaches further, so both answer something"
        );
    }

    #[test]
    fn nothing_kept_contains_anything_else_kept() {
        let (mut scene, fill) = surface(1.0);
        // Nested outwards, so every one of them contains the one pushed before it and the set has
        // to collapse to the last.
        for step in (0..6).rev() {
            let inset = step as f32 * 4.0;
            let side = 100.0 - inset * 2.0;
            scene.push_quad(Quad::filled(rect(inset, inset, side, side), fill));
        }
        assert_eq!(
            covers(&mut scene),
            vec![inset()],
            "each one contains the one before it, so the outermost is the whole answer"
        );
    }

    #[test]
    fn a_backdrop_stops_it_answering_at_all() {
        let (mut scene, fill) = surface(1.0);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 100.0, 100.0), fill));
        let blur = smallvec::smallvec![crate::group::Filter::Blur(2.0)];
        scene.push_backdrop(crate::group::BackdropFilter::new(
            rect(0.0, 0.0, 50.0, 50.0),
            blur,
        ));
        assert!(
            covers(&mut scene).is_empty(),
            "a backdrop reads what is beneath it, so every pixel it reads has to have been \
             written by this frame and no erasure may be left out"
        );
    }
}
