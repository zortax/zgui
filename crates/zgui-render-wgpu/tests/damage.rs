//! Redrawing part of a frame has to produce the frame.
//!
//! Every assertion here is the same one: a frame drawn against the rectangles that changed is
//! byte-identical to the same frame drawn against the whole surface. That is the only property
//! that makes redrawing part of a frame shippable, because every rectangle that was reported too
//! small becomes an immediate failure here instead of an intermittent trail on somebody's screen.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Pixels, WgpuRenderer};
use zgui_scene::{
    BackdropFilter, Filter, GroupBoundary, MarkFlags, MarkItem, MarkPayload, PaintRef, Quad, Scene,
};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// One thing a fixture draws.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Item {
    /// Where it is, in device pixels.
    bounds: (f32, f32, f32, f32),
    /// What colour it is.
    color: [u8; 3],
    /// The deviation of the blur over what lies beneath it, or `None` for a plain rectangle.
    frost: Option<f32>,
    /// How far the coordinate system this is drawn under is translated from the viewport's.
    ///
    /// The transform half of the fixture. A rectangle drawn under a coordinate system of its own
    /// keeps its own rectangle where it was and is moved by the matrix the shader resolves — which
    /// is what an animated transform is once it is a property write, and it is a different path
    /// through this file's assertion than moving the rectangle itself.
    shift: (f32, f32),
}

impl Item {
    /// A plain rectangle.
    fn plain(bounds: (f32, f32, f32, f32), color: [u8; 3]) -> Self {
        Self {
            bounds,
            color,
            frost: None,
            shift: (0.0, 0.0),
        }
    }

    /// The same rectangle, drawn under a coordinate system translated by `shift`.
    fn placed(mut self, shift: (f32, f32)) -> Self {
        self.shift = shift;
        self
    }

    /// A panel that blurs whatever is beneath it.
    fn frosted(bounds: (f32, f32, f32, f32), deviation: f32) -> Self {
        Self {
            bounds,
            color: [0; 3],
            frost: Some(deviation),
            shift: (0.0, 0.0),
        }
    }

    /// The rectangle this paints, in the coordinate system it is drawn under.
    fn local(&self) -> Rect<DevicePx, Device> {
        rect(self.bounds.0, self.bounds.1, self.bounds.2, self.bounds.3)
    }

    /// The rectangle this paints on the device.
    fn ink(&self) -> Rect<DevicePx, Device> {
        rect(
            self.bounds.0 + self.shift.0,
            self.bounds.1 + self.shift.1,
            self.bounds.2,
            self.bounds.3,
        )
    }

    /// Every pixel this reads, which is what it paints unless it filters what is beneath it.
    fn source(&self) -> Rect<DevicePx, Device> {
        match self.frost {
            Some(deviation) => {
                BackdropFilter::new(self.ink(), [Filter::Blur(deviation)].into_iter().collect())
                    .source
            }
            None => self.ink(),
        }
    }
}

/// Builds `items` into a finished scene.
fn build(items: &[Item]) -> Scene {
    build_with(items, false)
}

/// Builds `items` into a finished scene.
///
/// `keeping` marks the frosted panels as reading the copy kept from the last frame, which is what
/// the paint stage does once it has grown the damage on that understanding.
fn build_with(items: &[Item], keeping: bool) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    for (index, item) in items.iter().enumerate() {
        if let Some(deviation) = item.frost {
            let mut filter =
                BackdropFilter::new(item.ink(), [Filter::Blur(deviation)].into_iter().collect());
            if keeping {
                filter = filter.keeping_its_capture();
            }
            scene.push_backdrop(filter);
        } else {
            let paint = scene.paints.add(zgui_scene::Paint::Solid(opaque(
                item.color[0],
                item.color[1],
                item.color[2],
            )));
            // A shifted item establishes a coordinate system of its own, named after its position
            // in the fixture so that the same item is the same name from one frame to the next —
            // which is what makes moving it a write into the node rather than a new identity.
            let mut quad = Quad::filled(item.local(), paint);
            if item.shift != (0.0, 0.0) {
                let owner = zgui_scene::PropertyOwner::new(index as u64 + 2)
                    .expect("a handle is never the empty word");
                let own = zgui_scene::OwnSpace::of(
                    Some(zgui_geom::Matrix4::translation(
                        item.shift.0,
                        item.shift.1,
                        0.0,
                    )),
                    None,
                    false,
                );
                let viewport = scene.spatial.viewport();
                quad = quad.transformed(scene.spatial.space_of(viewport, owner, own));
            }
            scene.push_quad(quad);
        }
    }
    scene.finish(&DamageSet::full());
    scene
}

/// A fractional rectangle grown to whole device pixels.
fn whole(bounds: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    Rect::from_corners(
        Point::new(
            bounds.left().0.floor() as i32 - 1,
            bounds.top().0.floor() as i32 - 1,
        ),
        Point::new(
            bounds.right().0.ceil() as i32 + 1,
            bounds.bottom().0.ceil() as i32 + 1,
        ),
    )
}

/// The damage between two states of a fixture, expanded for everything that reads outside itself.
///
/// The expansion is the paint stage's job and this is the shape of it: a composite that samples
/// beyond what it writes has to have every pixel it samples inside a rectangle this frame is
/// redrawing, or it reads the previous frame's composite — which for a backdrop already contains
/// its own output, so the panel smears a little further every frame. It runs to a fixpoint,
/// because one grown rectangle can reach another composite's source.
fn damage_between(before: &[Item], after: &[Item]) -> DamageSet {
    let mut damage: DamageSet = DamageSet::new();
    let common = before.len().min(after.len());
    for index in 0..common {
        if before[index] != after[index] {
            damage.absorb(whole(before[index].ink()));
            damage.absorb(whole(after[index].ink()));
        }
    }
    for item in before.iter().skip(common).chain(after.iter().skip(common)) {
        damage.absorb(whole(item.ink()));
    }
    for _ in 0..=after.len() {
        let mut grew = false;
        for item in after {
            let source = whole(item.source());
            if source != whole(item.ink()) && damage.intersects(source) {
                let before_expansion = damage;
                damage.absorb(source);
                grew |= damage != before_expansion;
            }
        }
        if !grew {
            break;
        }
    }
    damage
}

/// Draws `items` with the damage between it and `previous`, and reads the result back.
fn draw_damaged(renderer: &mut WgpuRenderer, previous: &[Item], items: &[Item]) -> Pixels {
    let scene = build(items);
    let damage = damage_between(previous, items);
    let outcome = renderer.draw(&scene, &damage);
    assert!(outcome.retires_damage(), "{outcome:?}");
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}

/// The starting fixture: a few overlapping rectangles.
fn base() -> Vec<Item> {
    vec![
        Item::plain((8.0, 8.0, 48.0, 48.0), [200, 40, 40]),
        Item::plain((40.0, 24.0, 56.0, 40.0), [40, 200, 40]),
        Item::plain((16.0, 72.0, 96.0, 32.0), [40, 40, 200]),
    ]
}

/// A fixture whose one moving rectangle moves by its coordinate system rather than its own bounds.
///
/// The transform-animation steps. The rectangle's own bounds never change; what changes is the
/// matrix the coordinate system it is drawn under resolves to, which is what an animated transform
/// is once it is a property write. Damage is still the union of where the ink was and where it is,
/// and the assertion is the file's one assertion: the frame redrawn over that union is the frame
/// drawn whole.
fn transform_animation(step: i32) -> Vec<Item> {
    vec![
        Item::plain((0.0, 0.0, 128.0, 128.0), [235, 235, 235]),
        Item::plain((20.0, 20.0, 40.0, 40.0), [200, 60, 60]),
        Item::plain((10.0, 60.0, 36.0, 28.0), [30, 90, 210]).placed((step as f32 * 7.0, 0.0)),
        Item::plain((70.0, 70.0, 30.0, 30.0), [40, 170, 90]),
    ]
}

/// A fixture whose animating content sits *under* a panel that filters it.
///
/// The panel itself never changes, and neither does anything on its ancestor chain — which is
/// exactly why it is the fixture that matters: the damage that reaches it comes from the content
/// beneath it, and a rule that only looked at what changed would miss the region the panel reads.
fn backdrop_filter_over_animating_content(step: i32) -> Vec<Item> {
    frosted_over_animating_content(step, 5.0)
}

/// [`backdrop_filter_over_animating_content`] with a blur of `deviation`.
fn frosted_over_animating_content(step: i32, deviation: f32) -> Vec<Item> {
    vec![
        Item::plain((0.0, 0.0, 128.0, 128.0), [230, 230, 230]),
        Item::plain((8.0 + step as f32 * 3.0, 40.0, 32.0, 48.0), [20, 20, 200]),
        Item::frosted((32.0, 32.0, 64.0, 64.0), deviation),
    ]
}

/// A tiny deterministic generator, so a failing run is reproducible from its seed alone.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0 >> 33
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn coordinate(&mut self) -> f32 {
        (self.next() % 100) as f32
    }
}

#[test]
fn a_frame_drawn_against_its_damage_is_the_frame_drawn_against_the_whole_surface() {
    let Some((mut tracked, mut whole_surface)) = support::renderer_pair() else {
        return;
    };
    let mut rng = Rng(0x5eed);

    for seed in 0..24u32 {
        let mut items = base();
        // Both renderers start from the same picture, drawn in full.
        assert_eq!(
            present(&mut tracked, &build(&items))
                .max_difference(&present(&mut whole_surface, &build(&items))),
            0
        );

        for step in 0..6 {
            let previous = items.clone();
            match (rng.next() % 4, items.len()) {
                // Insert at a random position, which is what turns a structural mistake from
                // intermittent into immediate.
                (0, _) => {
                    let at = rng.below(items.len() + 1);
                    items.insert(
                        at,
                        Item::plain(
                            (rng.coordinate(), rng.coordinate(), 24.0, 24.0),
                            [(seed * 40) as u8, (step * 40) as u8, 90],
                        ),
                    );
                }
                // Remove at a random position.
                (1, count) if count > 1 => {
                    let at = rng.below(items.len());
                    items.remove(at);
                }
                // Move one.
                (2, count) if count > 0 => {
                    let at = rng.below(count);
                    items[at].bounds.0 = rng.coordinate();
                    items[at].bounds.1 = rng.coordinate();
                }
                // Recolour one, which moves nothing at all and is the commonest frame there is.
                (_, count) if count > 0 => {
                    let at = rng.below(count);
                    items[at].color = [(rng.next() % 256) as u8, 30, 30];
                }
                _ => continue,
            }

            let damaged = draw_damaged(&mut tracked, &previous, &items);
            let full = present(&mut whole_surface, &build(&items));
            assert_eq!(
                damaged.max_difference(&full),
                0,
                "seed {seed} step {step}: redrawing {:?} did not produce the whole frame",
                damage_between(&previous, &items).rects()
            );
        }
    }
}

#[test]
fn a_backdrop_over_animating_content_is_redrawn_without_smearing() {
    // The panel does not change and nothing on its ancestor chain does either, so the only thing
    // that puts the region it reads into the damage set is the expansion over what composites read
    // rather than over what changed. Without it the panel samples the previous frame's composite,
    // which already contains its own output, and a few frames of that is fog.
    let Some((mut tracked, mut whole_surface)) = support::renderer_pair() else {
        return;
    };
    let mut items = backdrop_filter_over_animating_content(0);
    present(&mut tracked, &build(&items));
    present(&mut whole_surface, &build(&items));

    for step in 1..8 {
        let previous = items.clone();
        items = backdrop_filter_over_animating_content(step);
        let damaged = draw_damaged(&mut tracked, &previous, &items);
        let full = present(&mut whole_surface, &build(&items));
        assert_eq!(
            damaged.max_difference(&full),
            0,
            "step {step}: the panel drifted from the frame drawn whole"
        );
    }
    assert!(
        !damage_between(
            &backdrop_filter_over_animating_content(6),
            &backdrop_filter_over_animating_content(7)
        )
        .is_full(),
        "the fixture must exercise a partial redraw, not fall back to the whole surface"
    );
}

#[test]
fn a_backdrop_with_a_small_or_zero_blur_is_redrawn_without_a_seam() {
    // A blur resamples through a half-resolution target, which reads two device pixels past its
    // kernel. A small blur reaches less than that, and a blur of zero has no kernel at all, so
    // these are the deviations where the read extent must count the resampling or skip the blur.
    for deviation in [0.0, 0.2] {
        let Some((mut tracked, mut whole_surface)) = support::renderer_pair() else {
            return;
        };
        let mut items = frosted_over_animating_content(0, deviation);
        present(&mut tracked, &build(&items));
        present(&mut whole_surface, &build(&items));
        for step in 1..8 {
            let previous = items.clone();
            items = frosted_over_animating_content(step, deviation);
            let damaged = draw_damaged(&mut tracked, &previous, &items);
            let full = present(&mut whole_surface, &build(&items));
            assert_eq!(
                damaged.max_difference(&full),
                0,
                "blur({deviation}), step {step}: the partial frame differs from the whole frame"
            );
        }
        assert!(
            !damage_between(
                &frosted_over_animating_content(6, deviation),
                &frosted_over_animating_content(7, deviation)
            )
            .is_full(),
            "the fixture must exercise a partial redraw"
        );
    }
}

#[test]
fn nothing_outside_a_damage_rectangle_is_touched() {
    // The mechanism, rather than the outcome: a frame whose damage names one corner has to leave
    // every other pixel exactly as the previous frame left it — including pixels whose content
    // changed in the display list. Without that, "redraw only what changed" is a description of
    // an intent rather than a thing the renderer does.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let items = base();
    let first = present(&mut renderer, &build(&items));

    let mut moved = items.clone();
    moved[2].color = [10, 10, 10];
    let corner: Rect<i32, Device> = Rect::new(Point::new(0, 0), Size::new(16, 16));
    let mut damage: DamageSet = DamageSet::new();
    damage.absorb(corner);
    let scene = build(&moved);
    renderer.draw(&scene, &damage);
    let second = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");

    assert_eq!(
        second.rgba(80, 80),
        first.rgba(80, 80),
        "a pixel outside the damaged corner still holds the previous frame"
    );
    assert_eq!(
        second.rgba(8, 8),
        first.rgba(8, 8),
        "and inside it, where nothing changed, it holds the same thing"
    );

    // Now damage the rectangle the changed item is in, and it changes.
    let mut wide: DamageSet = DamageSet::new();
    wide.absorb(whole(moved[2].ink()));
    renderer.draw(&scene, &wide);
    let third = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_ne!(
        third.rgba(80, 80),
        first.rgba(80, 80),
        "the change was invisible only because nothing had damaged it"
    );
}

#[test]
fn a_group_inside_a_damage_rectangle_is_composited_from_what_the_rectangle_holds() {
    // A group's own target is scratch, so it is redrawn per damage rectangle rather than kept —
    // and the composite is scissored to the rectangle, so a group straddling one contributes only
    // the part inside it. Drawing the same frame in two rectangles has to give the same picture as
    // drawing it in one.
    let Some((mut split, mut once)) = support::renderer_pair() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    let black = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let boundary = GroupBoundary::start(
        rect(16.0, 16.0, 96.0, 96.0),
        0.5,
        zgui_scene::peniko::BlendMode::default(),
        Default::default(),
    );
    scene.push_group(boundary.clone());
    scene.push_quad(Quad::filled(rect(16.0, 16.0, 96.0, 96.0), black));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());

    let mut halves: DamageSet = DamageSet::new();
    halves.absorb(Rect::new(Point::new(0, 0), Size::new(SIDE, 64)));
    halves.absorb(Rect::new(Point::new(0, 64), Size::new(SIDE, 64)));
    assert_eq!(halves.len(), 2, "the two halves are disjoint rectangles");

    split.draw(&scene, &halves);
    let in_halves = split
        .read_presented()
        .expect("a stand-in surface can be read back");
    let in_one = present(&mut once, &scene);
    assert_eq!(
        in_halves.max_difference(&in_one),
        0,
        "the same group composited in two rectangles is the same picture"
    );
}

/// Lines and rectangles stacked in the top left, the same stack turned in the bottom right, and a
/// blurred group whose content sits on both sides of the top-left rectangle's edge.
///
/// Each layer of a stack alternates kind, and the top layer of each is a rectangle `tint` colours.
fn culled_corners(tint: [u8; 3]) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    let grey = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(90, 90, 90)));
    let tinted = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(tint[0], tint[1], tint[2])));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    // A turn of 0.3 rad about the centre of the bottom-right stack.
    let (sin, cos) = 0.3f32.sin_cos();
    let owner = zgui_scene::PropertyOwner::new(9).expect("a handle is never the empty word");
    let turn = zgui_geom::Matrix4::from_columns([
        [cos, sin, 0.0, 0.0],
        [-sin, cos, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [96.0, 96.0, 0.0, 1.0],
    ]);
    let viewport = scene.spatial.viewport();
    let turned = scene.spatial.space_of(
        viewport,
        owner,
        zgui_scene::OwnSpace::of(Some(turn), None, false),
    );
    // The turned stack starts with a line, so no layer of it shares a batch with the other stack.
    for (origin, space, layers) in [((8.0, 8.0), None, 5), ((-16.0, -16.0), Some(turned), 6)] {
        for layer in 0..layers {
            let top = layer + 1 == layers;
            let at = rect(
                origin.0,
                origin.1 + 3.0 * layer as f32,
                32.0,
                if top { 12.0 } else { 8.0 },
            );
            if (layer + layers) % 2 == 0 {
                let mut line = zgui_scene::Decoration::new(
                    at,
                    2.0,
                    opaque(200, 30, 30),
                    zgui_scene::prim::DecorationStyle::Solid,
                );
                if let Some(space) = space {
                    line.transform = space.index();
                }
                scene.push_decoration(line);
            } else {
                let paint = if top { tinted } else { grey };
                let mut quad = Quad::filled(at, paint);
                if let Some(space) = space {
                    quad = quad.transformed(space);
                }
                scene.push_quad(quad);
            }
        }
    }
    // The group's line lies past the top-left rectangle, and its blur reaches into it.
    let boundary = GroupBoundary::start(
        rect(0.0, 48.0, 64.0, 24.0),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [Filter::Blur(3.0)].into_iter().collect(),
    );
    scene.push_group(boundary.clone());
    scene.push_quad(Quad::filled(rect(4.0, 52.0, 16.0, 8.0), tinted));
    scene.push_decoration(zgui_scene::Decoration::new(
        rect(44.0, 52.0, 12.0, 6.0),
        2.0,
        opaque(30, 30, 200),
        zgui_scene::prim::DecorationStyle::Solid,
    ));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_frame_with_batches_left_out_of_a_rectangle_is_the_frame_drawn_whole() {
    // With several rectangles, a batch that writes nothing inside one is not drawn into it. The
    // picture is the one property that must not notice.
    let Some((mut split, mut once)) = support::renderer_pair() else {
        return;
    };
    present(&mut split, &culled_corners([40, 160, 40]));
    let after = culled_corners([160, 40, 160]);

    let mut corners: DamageSet = DamageSet::new();
    corners.absorb(Rect::new(Point::new(0, 0), Size::new(42, 80)));
    corners.absorb(Rect::new(Point::new(68, 68), Size::new(56, 56)));
    assert_eq!(corners.len(), 2, "two disjoint rectangles");

    let culled = {
        let _turn = zgui_profile::counter::exclusive();
        let before = zgui_profile::counter::get(zgui_profile::Counter::BatchesCulled);
        let outcome = split.draw(&after, &corners);
        assert!(outcome.retires_damage(), "{outcome:?}");
        zgui_profile::counter::get(zgui_profile::Counter::BatchesCulled) - before
    };
    let in_corners = split
        .read_presented()
        .expect("a stand-in surface can be read back");
    let whole_frame = present(&mut once, &after);
    assert_eq!(
        in_corners.max_difference(&whole_frame),
        0,
        "the corners redrawn with batches left out are the frame drawn whole"
    );
    if zgui_profile::COUNTERS_ENABLED {
        assert!(
            culled > 0,
            "the fixture must leave batches out of a rectangle"
        );
    }
}

#[test]
fn a_group_inside_a_filtered_group_is_composited_over_all_of_what_the_filter_reads() {
    // A composite lands in whatever is beneath the group, and what may be written there is that
    // target's own region — the damage rectangle only when the target beneath *is* the composed
    // one. Cutting an inner group's composite down to the damage rectangle leaves the outer
    // group's filter reading texels the inner group never wrote, so the filtered pixels come out
    // differently depending on how much of the frame was being redrawn. Nothing else here nests a
    // group inside a filtered one, which is why this is its own fixture rather than another seed.
    let Some((mut split, mut once)) = support::renderer_pair() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    let black = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let outer = GroupBoundary::start(
        rect(16.0, 16.0, 96.0, 96.0),
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [Filter::Blur(4.0)].into_iter().collect(),
    );
    let inner = GroupBoundary::start(
        rect(24.0, 24.0, 80.0, 80.0),
        0.5,
        zgui_scene::peniko::BlendMode::default(),
        Default::default(),
    );
    scene.push_group(outer.clone());
    scene.push_group(inner.clone());
    scene.push_quad(Quad::filled(rect(24.0, 24.0, 80.0, 80.0), black));
    scene.push_group(inner.end());
    scene.push_group(outer.end());
    scene.finish(&DamageSet::full());

    // Two disjoint halves, so the boundary between them runs straight through the blurred group.
    let mut halves: DamageSet = DamageSet::new();
    halves.absorb(Rect::new(Point::new(0, 0), Size::new(SIDE, 64)));
    halves.absorb(Rect::new(Point::new(0, 64), Size::new(SIDE, 64)));
    assert_eq!(halves.len(), 2, "the two halves are disjoint rectangles");

    split.draw(&scene, &halves);
    let in_halves = split
        .read_presented()
        .expect("a stand-in surface can be read back");
    let in_one = present(&mut once, &scene);
    assert_eq!(
        in_halves.max_difference(&in_one),
        0,
        "the nested group blurred the same whichever rectangles were redrawn"
    );
}

#[test]
fn a_target_that_was_thrown_away_is_redrawn_whole_however_small_the_next_damage_set_is() {
    // The composed target is what makes redrawing part of a frame legal, so a resize that crosses
    // a size class and reallocates it leaves nothing for the next frame to rely on. Widening that
    // frame's damage to the whole surface is the mechanism; without it every pixel outside the
    // damage set is whatever an uninitialised texture holds.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let scene = |side: i32| {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(side, side));
        let white = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
        let red = scene
            .paints
            .add(zgui_scene::Paint::Solid(opaque(200, 40, 40)));
        scene.push_quad(Quad::filled(
            rect(0.0, 0.0, side as f32, side as f32),
            white,
        ));
        scene.push_quad(Quad::filled(rect(8.0, 8.0, 40.0, 40.0), red));
        scene.finish(&DamageSet::full());
        scene
    };
    present(&mut renderer, &scene(SIDE));

    // 128 and 300 are in different size classes, so the target is thrown away rather than reused.
    renderer.configure(RenderTarget::new(Size::new(300, 300), Scale::new(1.0)));
    let mut corner: DamageSet = DamageSet::new();
    corner.absorb(Rect::new(Point::new(0, 0), Size::new(16, 16)));
    let outcome = renderer.draw(&scene(300), &corner);
    assert_eq!(
        outcome.stats().map(|stats| stats.damage_px),
        Some(300 * 300),
        "the frame after a reallocation redraws all of the surface, not its damage set"
    );
    let after_resize = renderer.read_composed();

    renderer.draw(&scene(300), &DamageSet::full());
    assert_eq!(
        after_resize.max_difference(&renderer.read_composed()),
        0,
        "and what it left behind is the frame drawn whole"
    );
}

#[test]
fn a_transform_written_after_the_frame_was_ordered_redraws_to_the_frame_drawn_whole() {
    // The transform-animation steps, and the reason they belong in this file rather than beside the
    // tier that produces them: a moved coordinate system moves every primitive under it without any
    // of them being touched, so the only thing standing between that and a trail of pixels is the
    // damage the move reports. Redrawing over it has to produce the frame.
    let Some((mut tracked, mut whole_surface)) = support::renderer_pair() else {
        return;
    };
    let mut items = transform_animation(0);
    assert_eq!(
        present(&mut tracked, &build(&items))
            .max_difference(&present(&mut whole_surface, &build(&items))),
        0
    );

    for step in 1..10 {
        let previous = items.clone();
        items = transform_animation(step);
        let damaged = draw_damaged(&mut tracked, &previous, &items);
        let full = present(&mut whole_surface, &build(&items));
        assert_eq!(
            damaged.max_difference(&full),
            0,
            "step {step}: a written transform left the surface unlike the frame drawn whole"
        );
    }
    assert!(
        !damage_between(&transform_animation(8), &transform_animation(9)).is_full(),
        "the fixture must exercise a partial redraw, not fall back to the whole surface"
    );
}

#[test]
fn a_transform_that_moves_nothing_else_still_damages_what_it_moved() {
    // The non-vacuity half. Every rectangle in the fixture keeps its own bounds from one step to
    // the next, so a damage rule that only compared bounds would report nothing at all — and the
    // assertion above would then pass while redrawing an empty set on a surface that already held
    // the right picture.
    let damage = damage_between(&transform_animation(3), &transform_animation(4));
    assert!(
        !damage.is_empty(),
        "the step moved a coordinate system and damaged nothing"
    );
}

#[test]
fn a_frame_the_renderer_widens_paints_a_union_overlap_once() {
    // A fresh renderer redraws its first frame whole, whatever damage it is handed. The scene plans
    // its bins for a corner the discs do not reach, so the frame that draws them plans its own.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let translucent = PaintRef::solid(scene.paints.solid(Color::srgb_u8(200, 60, 20, 128)));
    let discs = MarkPayload {
        discs: vec![[48.0, 64.0, 24.0, 0.0], [80.0, 64.0, 24.0, 0.0]],
        ..MarkPayload::default()
    };
    let mut union = MarkItem::new(rect(24.0, 40.0, 80.0, 48.0), translucent, discs.counts());
    union.flags |= MarkFlags::UNION;
    scene.push_marks(union, Arc::new(discs));
    let mut corner = DamageSet::new();
    corner.absorb(Rect::new(Point::new(0, 0), Size::new(16, 16)));
    scene.finish(&corner);
    assert!(scene.mark_plan().is_empty(), "the corner reaches no disc");

    let outcome = renderer.draw(&scene, &corner);
    assert_eq!(
        outcome.stats().map(|stats| stats.damage_px),
        Some((SIDE * SIDE) as u64),
        "the first frame redraws all of the surface"
    );
    let pixels = renderer.read_composed();
    let single = pixels.rgba(36, 64)[3];
    let overlap = pixels.rgba(64, 64)[3];
    assert!((127..=129).contains(&single), "one disc: {single}");
    assert!(
        overlap.abs_diff(single) <= 1,
        "the overlap paints alpha once ({overlap}), as one disc does ({single}), never twice"
    );
}

/// A half-transparent square drawn through a group, under a coordinate system shifted by `shift`,
/// or drawn at the shifted place with no coordinate system at all when `flat`.
fn isolated(shift: f32, flat: bool) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let red = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(200, 30, 30)));
    let (offset, space) = if flat {
        (shift, None)
    } else {
        let owner = zgui_scene::PropertyOwner::new(9).expect("a handle");
        let own = zgui_scene::OwnSpace::of(
            Some(zgui_geom::Matrix4::translation(shift, 0.0, 0.0)),
            None,
            false,
        );
        let viewport = scene.spatial.viewport();
        (0.0, Some(scene.spatial.space_of(viewport, owner, own)))
    };
    let bounds = rect(20.0 + offset, 20.0, 30.0, 30.0);
    let mut group = GroupBoundary::start(
        bounds,
        0.5,
        zgui_scene::peniko::BlendMode::default(),
        Default::default(),
    );
    group.transform = space;
    scene.push_group(group.clone());
    let mut quad = Quad::filled(bounds, red);
    if let Some(space) = space {
        quad = quad.transformed(space);
    }
    scene.push_quad(quad);
    scene.push_group(group.end());
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_group_is_composited_where_its_coordinate_system_now_is() {
    // A group's bounds are in its own coordinate system, so a write that moves the system moves
    // the region the group is drawn and composited over with it. Read on the device as they were
    // emitted, they cut the content off where the system has moved away from.
    let Some((mut moved, mut flat)) = support::renderer_pair() else {
        return;
    };
    for shift in [0.0, 6.0, 40.0] {
        let drawn = present(&mut moved, &isolated(shift, false));
        let expected = present(&mut flat, &isolated(shift, true));
        assert_eq!(
            drawn.max_difference(&expected),
            0,
            "shifted by {shift}: the group under the system differs from the group drawn there"
        );
    }
}

/// Stripes under a blurring panel, drawn under a coordinate system shifted by `shift`, or drawn at
/// the shifted place with no coordinate system at all when `flat`.
fn frosted_in_space(shift: f32, flat: bool) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let black = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    for stripe in 0..(SIDE / 8) {
        scene.push_quad(Quad::filled(
            rect(stripe as f32 * 8.0, 0.0, 4.0, SIDE as f32),
            black,
        ));
    }
    let (offset, space) = if flat {
        (shift, None)
    } else {
        let owner = zgui_scene::PropertyOwner::new(9).expect("a handle");
        let own = zgui_scene::OwnSpace::of(
            Some(zgui_geom::Matrix4::translation(shift, 0.0, 0.0)),
            None,
            false,
        );
        let viewport = scene.spatial.viewport();
        (0.0, Some(scene.spatial.space_of(viewport, owner, own)))
    };
    scene.push_backdrop(
        BackdropFilter::new(
            rect(12.0 + offset, 12.0, 24.0, 24.0),
            [Filter::Blur(3.0)].into_iter().collect(),
        )
        .in_space(space),
    );
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_backdrop_is_drawn_where_its_coordinate_system_now_is() {
    // A backdrop's rectangles are in its own coordinate system, so a write that moves the system
    // moves what the filter reads and writes with it. Read on the device as they were emitted,
    // they blur the place the system has moved away from.
    let Some((mut moved, mut flat)) = support::renderer_pair() else {
        return;
    };
    for shift in [0.0, 6.0, 20.0] {
        let drawn = present(&mut moved, &frosted_in_space(shift, false));
        let expected = present(&mut flat, &frosted_in_space(shift, true));
        assert_eq!(
            drawn.max_difference(&expected),
            0,
            "shifted by {shift}: the backdrop under the system differs from the backdrop drawn there"
        );
    }
}

/// The damage a frame owes when the frosted panel reads the copy kept from the last one.
///
/// The panel's own read region is deliberately *not* absorbed — that is the whole point of keeping
/// a copy. What is absorbed is where the panel's answer changed, which is what moved beneath it
/// grown by how far the blur carries a pixel.
fn damage_keeping(before: &[Item], after: &[Item]) -> DamageSet {
    let mut damage: DamageSet = DamageSet::new();
    for (was, is) in before.iter().zip(after) {
        if was != is {
            damage.absorb(whole(was.ink()));
            damage.absorb(whole(is.ink()));
        }
    }
    for (panel, deviation) in after
        .iter()
        .filter_map(|item| item.frost.map(|f| (item, f)))
    {
        let bounds = whole(panel.ink());
        let reach = (Filter::BLUR_EXTENT * deviation).ceil() as i32;
        for moved in damage.rects().to_vec() {
            let Some(changed) = moved
                .outset(zgui_geom::Edges::uniform(reach))
                .intersection(bounds)
            else {
                continue;
            };
            damage.absorb(changed);
        }
    }
    damage
}

#[test]
fn a_backdrop_that_keeps_its_capture_draws_the_frame_drawn_whole() {
    // The exactness claim behind the kept copy. Outside the damage that copy holds an *earlier*
    // frame's composite, and the claim is that at a pixel no frame since has redrawn that is the
    // same picture — so a panel filtering it produces, to the last level, the frame that redrew
    // everything. Anything less would be a cheaper blur rather than the same one.
    let Some((mut tracked, mut whole_surface)) = support::renderer_pair() else {
        return;
    };
    let mut items = backdrop_filter_over_animating_content(0);
    // The first frame renews the copy, exactly as the expansion says it must: there is nothing
    // kept yet to read.
    present(&mut tracked, &build(&items));
    present(&mut whole_surface, &build(&items));

    for step in 1..8 {
        let previous = items.clone();
        items = backdrop_filter_over_animating_content(step);
        let damage = damage_keeping(&previous, &items);
        assert!(
            !damage.is_full(),
            "step {step}: the fixture stopped being partial"
        );
        let outcome = tracked.draw(&build_with(&items, true), &damage);
        assert!(outcome.retires_damage(), "{outcome:?}");
        let damaged = tracked
            .read_presented()
            .expect("a stand-in surface can be read back");
        let full = present(&mut whole_surface, &build(&items));
        assert_eq!(
            damaged.max_difference(&full),
            0,
            "step {step}: the kept copy drew a different picture from the frame drawn whole"
        );
    }
}

#[test]
fn keeping_the_capture_is_what_stops_the_damage_reaching_the_panel() {
    // The measurement the change was made for, as an assertion: the same two frames cost the
    // panel's whole read region when the copy is renewed and only what moved when it is kept.
    let before = backdrop_filter_over_animating_content(3);
    let after = backdrop_filter_over_animating_content(4);
    let renewed: i64 = damage_between(&before, &after)
        .area()
        .expect("a bounded set");
    let kept: i64 = damage_keeping(&before, &after)
        .area()
        .expect("a bounded set");
    assert!(
        kept * 2 < renewed,
        "keeping the copy cost {kept} px against renewing it at {renewed}"
    );
}
