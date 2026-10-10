use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits};
use zgui_color::Color;
use zgui_geom::Affine2;
use zgui_scene::VectorId;
use zgui_scene::kurbo::{Affine, BezPath, Rect, Shape as _};
use zgui_scene::peniko;

use super::{LayerAnswer, LayerFallback, LayerRequest, VectorLayerCache};
use crate::content::vectors::Drawing;
use crate::emit::vector::ShapePaint;

/// A cache and an atlas, one frame at a time.
struct Fixture {
    cache: VectorLayerCache,
    atlas: Atlas,
}

impl Fixture {
    fn new(ready: bool) -> Self {
        let mut fixture = Self {
            cache: VectorLayerCache::default(),
            atlas: Atlas::new(AtlasLimits::default()),
        };
        fixture.cache.set_raster_ready(ready);
        fixture.frame();
        fixture
    }

    /// Ends the frame and starts the next.
    fn frame(&mut self) {
        self.cache.end_frame(&mut self.atlas);
        self.atlas.begin_frame();
        self.cache.begin_frame();
    }

    fn ask(&mut self, owner: u32, revision: u64, drawing: &Drawing, scale: f32) -> LayerAnswer {
        self.ask_with(owner, revision, drawing, paint(Color::BLACK), spatial(scale))
    }

    fn ask_with(
        &mut self,
        owner: u32,
        revision: u64,
        drawing: &Drawing,
        paint: ShapePaint,
        spatial: Affine2,
    ) -> LayerAnswer {
        let mut named = Vec::new();
        self.cache.layer(
            &mut self.atlas,
            LayerRequest {
                owner: VectorId(owner),
                revision,
                drawing,
                paint,
                spatial: Some(spatial),
            },
            &mut named,
            &mut |_| Vec::new(),
        )
    }
}

fn spatial(scale: f32) -> Affine2 {
    Affine2::new(scale, 0.0, 0.0, scale, 0.0, 0.0)
}

fn paint(fill: Color) -> ShapePaint {
    ShapePaint {
        fill,
        stroke: None,
        stroke_width: 1.0,
    }
}

/// A drawing of one filled outline.
fn drawing(path: BezPath, ink: zgui_svg::Ink) -> Drawing {
    Drawing::fitted_shared(
        Arc::from(vec![zgui_svg::Shape {
            path: Arc::new(path),
            fill: Some(zgui_svg::Fill {
                paint: zgui_svg::Paint::Solid(ink),
                rule: peniko::Fill::NonZero,
            }),
            stroke: None,
            clips: Vec::new(),
        }]),
        Affine::IDENTITY,
    )
}

/// A 16 by 16 square in its own colour.
fn square() -> Drawing {
    drawing(
        Rect::new(0.0, 0.0, 16.0, 16.0).to_path(0.1),
        zgui_svg::Ink::Solid(Color::WHITE),
    )
}

/// A small outline of `segments` lines, estimated at about 1.5 µs each.
fn zigzag(segments: usize) -> Drawing {
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    for index in 0..segments {
        let x = 16.0 * index as f64 / segments as f64;
        path.line_to((x, if index % 2 == 0 { 16.0 } else { 0.0 }));
    }
    path.close_path();
    drawing(path, zgui_svg::Ink::Solid(Color::WHITE))
}

fn key(answer: LayerAnswer) -> AtlasKey {
    match answer {
        LayerAnswer::Sprite { key, .. } => key,
        other => panic!("expected a sprite, got {other:?}"),
    }
}

fn provisional(answer: LayerAnswer) -> bool {
    match answer {
        LayerAnswer::Sprite { provisional, .. } => provisional,
        other => panic!("expected a sprite, got {other:?}"),
    }
}

fn rasterised() -> u64 {
    zgui_profile::counter::get(zgui_profile::Counter::VectorLayersRasterised)
}

#[test]
fn equal_drawings_share_one_layer() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let other = shared.clone();
    let one = key(fixture.ask(1, 7, &shared, 1.0));
    let two = key(fixture.ask(2, 7, &other, 1.0));
    assert_eq!(one, two);
    assert_eq!(fixture.cache.len(), 1);
}

#[test]
fn an_inherited_drawing_keys_its_paint_and_an_own_coloured_one_does_not() {
    let mut fixture = Fixture::new(false);
    let inherited = drawing(
        Rect::new(0.0, 0.0, 16.0, 16.0).to_path(0.1),
        zgui_svg::Ink::Inherited { alpha: 1.0 },
    );
    let red = paint(Color::srgb(1.0, 0.0, 0.0, 1.0));
    let blue = paint(Color::srgb(0.0, 0.0, 1.0, 1.0));
    let a = key(fixture.ask_with(1, 1, &inherited, red, spatial(1.0)));
    let b = key(fixture.ask_with(2, 1, &inherited, blue, spatial(1.0)));
    assert_ne!(a, b, "the paint is part of what an inherited drawing looks like");

    let own = square();
    let c = key(fixture.ask_with(3, 2, &own, red, spatial(1.0)));
    let d = key(fixture.ask_with(4, 2, &own, blue, spatial(1.0)));
    assert_eq!(c, d, "a drawing in its own colours ignores the element's");
}

#[test]
fn a_scale_change_is_provisional_until_three_stable_frames() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let first = key(fixture.ask(1, 1, &shared, 1.0));
    for frame in 0..3 {
        fixture.frame();
        let answer = fixture.ask(1, 1, &shared, 1.5);
        assert!(provisional(answer), "frame {frame} stretches the raster it has");
        assert_eq!(key(answer), first);
    }
    fixture.frame();
    let settled = fixture.ask(1, 1, &shared, 1.5);
    assert!(!provisional(settled));
    assert_ne!(key(settled), first, "the settled scale has a raster of its own");

    // A change that keeps moving stays provisional for as long as it moves.
    for step in 0..10 {
        fixture.frame();
        let scale = 1.5 + 0.05 * (step + 1) as f32;
        assert!(provisional(fixture.ask(1, 1, &shared, scale)));
    }
}

#[test]
fn a_first_raster_while_cold_stays_in_the_cold_budget() {
    let mut fixture = Fixture::new(false);
    // About 3 ms each, so two fit the 8 ms of one frame and the third waits.
    let drawings: Vec<Drawing> = (0..3).map(|_| zigzag(2_000)).collect();
    let answers: Vec<LayerAnswer> = drawings
        .iter()
        .enumerate()
        .map(|(index, drawing)| fixture.ask(index as u32, index as u64, drawing, 1.0))
        .collect();
    assert!(matches!(answers[0], LayerAnswer::Sprite { .. }));
    assert!(matches!(answers[1], LayerAnswer::Sprite { .. }));
    assert!(matches!(answers[2], LayerAnswer::Defer { .. }));
    fixture.frame();
    assert!(matches!(
        fixture.ask(2, 2, &drawings[2], 1.0),
        LayerAnswer::Sprite { .. }
    ));
}

#[test]
fn a_deferred_drawing_rasterises_by_its_third_frame() {
    let mut fixture = Fixture::new(false);
    let late = zigzag(2_000);
    for frame in 0..3_u64 {
        // Each frame something else spends the cold budget first.
        for index in 0..3 {
            let other = zigzag(2_000);
            fixture.ask(10 + index, 100 + frame * 3 + u64::from(index), &other, 1.0);
        }
        let answer = fixture.ask(1, 1, &late, 1.0);
        if frame < 2 {
            assert!(matches!(answer, LayerAnswer::Defer { .. }), "frame {frame}");
        } else {
            assert!(matches!(answer, LayerAnswer::Sprite { .. }), "over the budget");
        }
        fixture.frame();
    }
}

#[test]
fn a_warm_first_raster_waits_two_stable_frames_unless_cheap() {
    let mut fixture = Fixture::new(true);
    let cheap = square();
    assert!(matches!(
        fixture.ask(1, 1, &cheap, 1.0),
        LayerAnswer::Sprite { .. }
    ));
    let costly = zigzag(100);
    for frame in 0..2 {
        assert_eq!(
            fixture.ask(2, 2, &costly, 1.0),
            LayerAnswer::Items(LayerFallback::Budget),
            "frame {frame}"
        );
        fixture.frame();
    }
    assert!(matches!(
        fixture.ask(2, 2, &costly, 1.0),
        LayerAnswer::Sprite { .. }
    ));
}

#[test]
fn upgrades_take_at_most_three_layers_and_two_ms_a_frame() {
    let mut fixture = Fixture::new(true);
    // About 0.6 ms each: three fit two milliseconds, and the fourth waits for the count.
    let drawings: Vec<Drawing> = (0..5).map(|_| zigzag(400)).collect();
    // Placed while cold, so every first raster is admitted at once.
    fixture.cache.set_raster_ready(false);
    for (index, drawing) in drawings.iter().enumerate() {
        key(fixture.ask(index as u32, index as u64, drawing, 1.0));
    }
    fixture.cache.set_raster_ready(true);
    for _ in 0..3 {
        fixture.frame();
        for (index, drawing) in drawings.iter().enumerate() {
            assert!(provisional(fixture.ask(index as u32, index as u64, drawing, 1.5)));
        }
    }
    fixture.frame();
    let before = rasterised();
    let exact = drawings
        .iter()
        .enumerate()
        .filter(|(index, drawing)| !provisional(fixture.ask(*index as u32, *index as u64, drawing, 1.5)))
        .count();
    assert_eq!(exact, 3);
    assert!(rasterised() - before >= 3);
    fixture.frame();
    let exact = drawings
        .iter()
        .enumerate()
        .filter(|(index, drawing)| !provisional(fixture.ask(*index as u32, *index as u64, drawing, 1.5)))
        .count();
    assert_eq!(exact, 5, "the other two settle on the next frame");

    // One upgrade costing more than the whole budget still settles, alone in its frame.
    fixture.frame();
    fixture.cache.set_raster_ready(false);
    let large = zigzag(2_000);
    key(fixture.ask(20, 20, &large, 1.0));
    let small = zigzag(400);
    key(fixture.ask(21, 21, &small, 1.0));
    fixture.cache.set_raster_ready(true);
    for _ in 0..3 {
        fixture.frame();
        assert!(provisional(fixture.ask(20, 20, &large, 1.5)));
        assert!(provisional(fixture.ask(21, 21, &small, 1.5)));
    }
    fixture.frame();
    assert!(!provisional(fixture.ask(20, 20, &large, 1.5)));
    assert!(provisional(fixture.ask(21, 21, &small, 1.5)), "the budget is spent");
}

#[test]
fn three_rasters_in_thirty_frames_demote_for_thirty() {
    let mut fixture = Fixture::new(true);
    let drawings: Vec<Drawing> = (0..40).map(|_| square()).collect();
    // A new source every frame is a first raster every frame.
    for (frame, drawing) in drawings.iter().take(3).enumerate() {
        assert!(matches!(
            fixture.ask(1, frame as u64, drawing, 1.0),
            LayerAnswer::Sprite { .. }
        ));
        fixture.frame();
    }
    for (frame, drawing) in drawings.iter().enumerate().skip(3).take(29) {
        assert_eq!(
            fixture.ask(1, frame as u64, drawing, 1.0),
            LayerAnswer::Items(LayerFallback::Demoted),
            "frame {frame}"
        );
        fixture.frame();
    }
    assert!(matches!(
        fixture.ask(1, 99, &drawings[39], 1.0),
        LayerAnswer::Sprite { .. }
    ));
}

#[test]
fn a_turned_or_mirrored_space_is_ineligible() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let turned = Affine2::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
    let mirrored = Affine2::new(-1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    for space in [turned, mirrored] {
        assert_eq!(
            fixture.ask_with(1, 1, &shared, paint(Color::BLACK), space),
            LayerAnswer::Items(LayerFallback::Ineligible)
        );
    }
}

#[test]
fn a_drawing_past_the_size_or_cost_cap_is_ineligible() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    // 16 units at 129 is 2064 texels a side.
    assert_eq!(
        fixture.ask(1, 1, &shared, 129.0),
        LayerAnswer::Items(LayerFallback::Ineligible)
    );
    // 1100 by 1100 texels is past four megabytes.
    assert_eq!(
        fixture.ask(1, 1, &shared, 68.75),
        LayerAnswer::Items(LayerFallback::Ineligible)
    );
    assert_eq!(
        fixture.ask(2, 2, &zigzag(6_000), 1.0),
        LayerAnswer::Items(LayerFallback::Ineligible)
    );
}

#[test]
fn only_two_scales_are_kept_per_source() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let mut keys = Vec::new();
    for scale in [1.0, 1.25, 1.5] {
        // Three frames stretch the scale before, and the fourth rasterises it.
        let mut answer = fixture.ask(1, 1, &shared, scale);
        for _ in 0..3 {
            fixture.frame();
            answer = fixture.ask(1, 1, &shared, scale);
        }
        assert!(!provisional(answer));
        keys.push(key(answer));
        fixture.frame();
    }
    fixture.frame();
    assert!(!fixture.atlas.contains(keys[0]), "the oldest scale goes");
    assert!(fixture.atlas.contains(keys[1]));
    assert!(fixture.atlas.contains(keys[2]));
    assert_eq!(fixture.cache.len(), 2);
}

#[test]
fn a_new_drawing_stretches_a_raster_its_source_has() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let first = key(fixture.ask(1, 1, &shared, 1.0));
    // A fragment rebuilt under a new transform asks under a new name.
    let answer = fixture.ask(2, 1, &shared, 1.5);
    assert!(provisional(answer));
    assert_eq!(key(answer), first);
}

#[test]
fn evicting_spares_held_layers() {
    let mut fixture = Fixture::new(false);
    let one = square();
    let two = square();
    let held = key(fixture.ask(1, 1, &one, 1.0));
    let free = key(fixture.ask(2, 2, &two, 1.0));
    fixture.atlas.retain(held);
    fixture.frame();
    let bytes = fixture.cache.bytes();
    assert_eq!(fixture.cache.pinned_bytes(&fixture.atlas), 0, "this frame drew neither");
    assert_eq!(fixture.cache.held_bytes(&fixture.atlas), bytes / 2);
    let freed = fixture.cache.evict(&mut fixture.atlas, u64::MAX);
    assert_eq!(freed, bytes / 2);
    assert!(fixture.atlas.contains(held));
    assert!(!fixture.atlas.contains(free));
    assert_eq!(fixture.cache.bytes(), bytes / 2);
}

#[test]
fn forgotten_tiles_leave_the_metadata() {
    let mut fixture = Fixture::new(false);
    let shared = square();
    let first = key(fixture.ask(1, 1, &shared, 1.0));
    assert!(fixture.atlas.remove(first));
    fixture.cache.forget_tiles(&[first]);
    assert_eq!(fixture.cache.len(), 0);
    assert_eq!(fixture.cache.bytes(), 0);
    fixture.frame();
    let again = key(fixture.ask(1, 1, &shared, 1.0));
    assert!(fixture.atlas.contains(again));
}
