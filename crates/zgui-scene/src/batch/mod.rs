//! Grouping the sorted primitives into the fewest draw calls that preserve their order.

use core::ops::Range;

use crate::id::DrawOrder;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;
use crate::shader::{ShaderId, ShaderParamsSlot};

/// What the merge orders primitives by: draw order first, then kind.
///
/// The kind is the tie-break, and it is a batching preference rather than a correctness mechanism —
/// two primitives at equal draw order are provably non-overlapping.
type SortKey = (DrawOrder, PrimitiveKind);

/// One draw call's worth of primitives.
///
/// A ranged batch is a contiguous range of its kind's *remap list* — see
/// [`Scene::remap`](crate::Scene::remap) — and a consumer reads the array through it. The arrays
/// themselves keep push order, so a primitive's position in one is stable for the frame however
/// the ordering falls out. The one-at-a-time variants carry the array index directly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Batch {
    /// Rounded rectangles.
    Quads(Range<usize>),
    /// Rectangles one application effect draws with one set of parameters.
    Shaded {
        /// The effect every rectangle of the batch is drawn by.
        shader: ShaderId,
        /// The parameter block every rectangle of the batch is drawn with.
        params: ShaderParamsSlot,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// Box shadows.
    Shadows(Range<usize>),
    /// Text decoration lines.
    Decorations(Range<usize>),
    /// Single-channel coverage sprites reading one texture.
    MonoSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// Three-channel coverage sprites reading one texture.
    SubpixelSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// Full-colour sprites reading one texture.
    ColorSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// One composite of a rasterisation pass, by its index in the pass plan.
    Vector(usize),
    /// One external texture, by its index in the scene's array.
    External(usize),
    /// One backdrop filter, by its index in the scene's array.
    Backdrop(usize),
    /// One group marker, by its index in the scene's array.
    ///
    /// Never merged with anything: a renderer has to change target at exactly this point, and a
    /// marker swallowed into a batch is a target switched at the wrong moment.
    Group(usize),
}

/// The scene's primitives, grouped into draw calls.
///
/// The iterator merges the arrays by `(draw order, primitive kind)` and yields the longest run it
/// can take from one of them before another array's next primitive would have to come first. That
/// is the whole of batching: order is a total order, and a batch is a maximal contiguous run within
/// it.
///
/// Sprites break additionally on a change of texture, and application effects on a change of
/// effect or parameters, because a draw call binds one of each.
pub struct Batches<'scene> {
    /// The scene being walked.
    scene: &'scene Scene,
    /// How far each array has been consumed, indexed by [`PrimitiveKind`] position.
    cursors: [usize; PrimitiveKind::ALL.len()],
}

impl<'scene> Batches<'scene> {
    /// Groups `scene`'s primitives, which must already be finished.
    pub(crate) fn new(scene: &'scene Scene) -> Self {
        Self {
            scene,
            cursors: [0; PrimitiveKind::ALL.len()],
        }
    }

    /// The next primitive waiting in `kind`'s array, as a sort key.
    fn peek(&self, kind: PrimitiveKind) -> Option<SortKey> {
        let cursor = self.cursors[kind as usize];
        self.scene.order_at(kind, cursor).map(|order| (order, kind))
    }

    /// The two lowest waiting keys, which is all a merge needs.
    ///
    /// A scan rather than a sort: a dozen candidates is small enough that finding the two smallest
    /// in one pass beats sorting all of them, and the second smallest is exactly the bound the
    /// winning run may not cross.
    fn two_lowest(&self) -> (Option<SortKey>, Option<SortKey>) {
        let mut best: Option<SortKey> = None;
        let mut second: Option<SortKey> = None;
        for kind in PrimitiveKind::ALL {
            let Some(candidate) = self.peek(kind) else {
                continue;
            };
            if best.is_none_or(|held| candidate < held) {
                second = best;
                best = Some(candidate);
            } else if second.is_none_or(|held| candidate < held) {
                second = Some(candidate);
            }
        }
        (best, second)
    }
}

impl Iterator for Batches<'_> {
    type Item = Batch;

    fn next(&mut self) -> Option<Batch> {
        let (best, second) = self.two_lowest();
        let (_, kind) = best?;
        let limit = second.unwrap_or((DrawOrder::MAX, PrimitiveKind::GroupEnd));
        let start = self.cursors[kind as usize];
        let (batch, next) = self.scene.take_batch(kind, start, limit);
        self.cursors[kind as usize] = next;
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use crate::prim::ShadedQuad;
    use crate::shader::{ShaderId, ShaderParamsSlot};
    use crate::{Batch, Scene};
    use zgui_bits::DamageSet;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    /// A rectangle at `x`, wide enough to be its own non-overlapping column.
    fn at(x: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(0.0)),
            Size::new(DevicePx(8.0), DevicePx(8.0)),
        )
    }

    /// A finished scene holding `pushed` shaded rectangles.
    fn scene(pushed: &[(f32, u32, u32)]) -> Scene {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(256, 64));
        for (x, shader, params) in pushed {
            scene.push_shaded(ShadedQuad::new(
                at(*x),
                ShaderId(*shader),
                ShaderParamsSlot(*params),
            ));
        }
        scene.finish(&DamageSet::full());
        scene
    }

    #[test]
    fn rectangles_of_one_effect_with_one_block_are_one_draw() {
        let scene = scene(&[(0.0, 1, 3), (16.0, 1, 3), (32.0, 1, 3)]);
        let batches: Vec<Batch> = scene.batches().collect();
        assert_eq!(
            batches,
            vec![Batch::Shaded {
                shader: ShaderId(1),
                params: ShaderParamsSlot(3),
                range: 0..3,
            }],
            "one pipeline and one block is one draw call"
        );
    }

    #[test]
    fn a_change_of_effect_breaks_the_run() {
        let scene = scene(&[(0.0, 1, 3), (16.0, 2, 3)]);
        let batches: Vec<Batch> = scene.batches().collect();
        assert_eq!(batches.len(), 2, "two pipelines cannot be one draw call");
    }

    #[test]
    fn a_change_of_parameters_breaks_the_run() {
        let scene = scene(&[(0.0, 1, 3), (16.0, 1, 4)]);
        let batches: Vec<Batch> = scene.batches().collect();
        assert_eq!(batches.len(), 2, "two blocks cannot be one draw call");
    }

    /// The sort is what makes the run above contiguous: the rectangles are pushed interleaved and
    /// still come out as two draws rather than four.
    #[test]
    fn rectangles_pushed_interleaved_are_clustered_by_what_they_bind() {
        let scene = scene(&[(0.0, 1, 3), (16.0, 2, 3), (32.0, 1, 3), (48.0, 2, 3)]);
        let batches: Vec<Batch> = scene.batches().collect();
        assert_eq!(batches.len(), 2, "{batches:?}");
    }

    #[test]
    fn an_effect_never_shares_a_draw_with_an_ordinary_rectangle() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(256, 64));
        let fill = scene
            .paints
            .add(crate::Paint::Solid(zgui_color::Color::BLACK));
        scene.push_quad(crate::Quad::filled(at(0.0), fill));
        scene.push_shaded(ShadedQuad::new(at(16.0), ShaderId(1), ShaderParamsSlot(0)));
        scene.finish(&DamageSet::full());
        let batches: Vec<Batch> = scene.batches().collect();
        assert_eq!(batches.len(), 2, "{batches:?}");
    }
}
