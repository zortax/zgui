use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasLimits};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Affine2, Device, DevicePx, Point, Rect, Size};
use zgui_profile::Counter;
use zgui_scene::kurbo::{self, Affine, Shape as _};
use zgui_scene::{ColorSprite, Scene, VectorId, peniko};

use super::super::{LayerAnswer, LayerRequest, VectorLayerCache};
use super::{TILE, Tiles};
use crate::content::vectors::Drawing;
use crate::emit::vector::ShapePaint;

/// A cache and an atlas, one frame at a time.
struct Fixture {
    cache: VectorLayerCache,
    atlas: Atlas,
}

impl Fixture {
    fn new() -> Self {
        let mut fixture = Self {
            cache: VectorLayerCache::default(),
            atlas: Atlas::new(AtlasLimits::default()),
        };
        fixture.cache.set_raster_ready(true);
        fixture.frame();
        fixture
    }

    /// Ends the frame and starts the next.
    fn frame(&mut self) {
        self.cache.end_frame(&mut self.atlas);
        self.atlas.begin_frame();
        self.cache.begin_frame();
    }

    /// The answer for `drawing` under a scale of `scale` moved by `shift` device pixels.
    fn ask(
        &mut self,
        revision: u64,
        drawing: &Drawing,
        scale: f32,
        shift: [f32; 2],
    ) -> LayerAnswer {
        let mut named = Vec::new();
        self.cache.layer(
            &mut self.atlas,
            LayerRequest {
                owner: VectorId(1),
                revision,
                drawing,
                paint: ShapePaint {
                    fill: Color::BLACK,
                    stroke: None,
                    stroke_width: 1.0,
                },
                spatial: Some(Affine2::new(scale, 0.0, 0.0, scale, shift[0], shift[1])),
            },
            &mut named,
            &mut |_| Vec::new(),
        )
    }

    /// Pushes every tile of `tiles` at `scale` into a frame of `viewport`, settles it against
    /// `damage`, and returns the scene and what is owed.
    fn settle(
        &mut self,
        tiles: &Tiles,
        scale: f64,
        viewport: Size<i32, Device>,
        damage: &DamageSet,
    ) -> (Scene, Vec<Rect<i32, Device>>) {
        let mut scene = Scene::new();
        scene.begin_frame(viewport);
        for (name, path_rect) in tiles.sprites() {
            let device = Affine::scale(scale).transform_rect_bbox(path_rect);
            scene.push_color_sprite(ColorSprite::new(rect(device), name));
        }
        let owed = self
            .cache
            .settle(&mut self.atlas, &mut scene, damage, &mut |_| Vec::new());
        assert!(
            !scene.has_unresolved_resources(),
            "every tile sprite is placed or blank"
        );
        (scene, owed)
    }
}

fn rect(rect: kurbo::Rect) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(rect.x0 as f32), DevicePx(rect.y0 as f32)),
        Size::new(
            DevicePx(rect.width() as f32),
            DevicePx(rect.height() as f32),
        ),
    )
}

/// A solid shape over `area`.
fn solid(area: kurbo::Rect, color: Color) -> zgui_svg::Shape {
    zgui_svg::Shape {
        path: Arc::new(area.to_path(0.1)),
        fill: Some(zgui_svg::Fill {
            paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(color)),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// Shapes over 0..`side` in both axes: a ground and one square in each quarter.
fn quarters(side: f64) -> Vec<zgui_svg::Shape> {
    let half = side / 2.0;
    let mut shapes = vec![solid(
        kurbo::Rect::new(0.0, 0.0, side, side),
        Color::srgb(0.1, 0.1, 0.2, 1.0),
    )];
    for (x, y) in [(0.0, 0.0), (half, 0.0), (0.0, half), (half, half)] {
        shapes.push(solid(
            kurbo::Rect::new(x + 2.0, y + 2.0, x + half - 2.0, y + half - 2.0),
            Color::srgb(0.9, 0.4, 0.2, 1.0),
        ));
    }
    shapes
}

fn tiles(answer: LayerAnswer) -> Tiles {
    match answer {
        LayerAnswer::Tiles { tiles, .. } => tiles,
        other => panic!("expected tiles, got {other:?}"),
    }
}

fn get(counter: Counter) -> u64 {
    zgui_profile::counter::get(counter)
}

#[test]
fn a_drawing_past_the_size_cap_is_tiled() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(quarters(100.0)), Affine::IDENTITY);
    // 100 units at 30 is 3000 texels a side.
    let tiled = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    let per_side = 3002_u32.div_ceil(TILE) as usize + 1;
    assert!(tiled.len() >= 36 && tiled.len() <= per_side * per_side);
    let covered: f64 = tiled.sprites().map(|(_, path_rect)| path_rect.area()).sum();
    assert!(
        (covered - 100.0 * 100.0).abs() < 1.0,
        "the tiles cover the ink once: {covered}"
    );
    // A small drawing is never tiled.
    assert!(!matches!(
        fixture.ask(2, &drawing, 2.0, [0.0, 0.0]),
        LayerAnswer::Tiles { .. }
    ));
}

#[test]
fn a_translation_keeps_every_tile() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(quarters(100.0)), Affine::IDENTITY);
    let first = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    fixture.frame();
    let moved = tiles(fixture.ask(1, &drawing, 30.0, [137.25, -40.5]));
    assert_eq!(first, moved, "one source");
    let names = |tiles: &Tiles| tiles.sprites().collect::<Vec<_>>();
    assert_eq!(names(&first), names(&moved));
}

#[test]
fn settle_rasterises_only_the_needed_tiles() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(quarters(100.0)), Affine::IDENTITY);
    let tiled = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    let viewport = Size::new(1200, 900);
    // The top-left corner only: one tile, and the tiles around it ahead while budget is left.
    let mut damage = DamageSet::new();
    damage.absorb(Rect::new(Point::new(10, 10), Size::new(100, 100)));
    let before = get(Counter::VectorLayerTilesRasterised);
    let (scene, owed) = fixture.settle(&tiled, 30.0, viewport, &damage);
    let rasterised = get(Counter::VectorLayerTilesRasterised) - before;
    assert!(owed.is_empty());
    assert!(rasterised >= 1, "the damaged tile");
    assert!(
        rasterised <= 4,
        "the damaged tile and its neighbours ahead, no more: {rasterised}"
    );
    let placed = scene
        .primitives
        .color_sprites
        .iter()
        .filter(|sprite| sprite.bounds != [0.0; 4])
        .count();
    assert_eq!(placed as u64, rasterised, "the rest draw nothing");

    // The same frame again rasterises nothing.
    fixture.frame();
    let before = get(Counter::VectorLayerTilesRasterised);
    fixture.settle(&tiled, 30.0, viewport, &damage);
    assert_eq!(get(Counter::VectorLayerTilesRasterised), before);
}

#[test]
fn a_deferred_tile_is_owed_and_rasterised_by_its_third_frame() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    // Many segments per tile, so each tile costs more than the frame's budget allows twice.
    let mut shapes = Vec::new();
    for row in 0..10 {
        for column in 0..10 {
            let mut path = kurbo::BezPath::new();
            let (x, y) = (f64::from(column) * 10.0, f64::from(row) * 10.0);
            path.move_to((x, y));
            for step in 0..400 {
                let t = f64::from(step) / 400.0;
                path.line_to((x + 10.0 * t, y + if step % 2 == 0 { 0.0 } else { 9.0 }));
            }
            path.close_path();
            shapes.push(zgui_svg::Shape {
                path: Arc::new(path),
                fill: Some(zgui_svg::Fill {
                    paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(Color::WHITE)),
                    rule: peniko::Fill::NonZero,
                }),
                stroke: None,
                clips: Vec::new(),
            });
        }
    }
    let drawing = Drawing::fitted_shared(Arc::from(shapes), Affine::IDENTITY);
    let tiled = tiles(fixture.ask(1, &drawing, 25.0, [0.0, 0.0]));
    let viewport = Size::new(1536, 1024);
    let damage = DamageSet::full();
    let mut deferred_frames = 0;
    let mut first_owed = Vec::new();
    for frame in 0..3 {
        let before = get(Counter::VectorLayerTilesDeferred);
        let (scene, owed) = fixture.settle(&tiled, 25.0, viewport, &damage);
        if frame == 0 {
            first_owed = owed.clone();
        }
        if get(Counter::VectorLayerTilesDeferred) > before {
            deferred_frames += 1;
            assert!(!owed.is_empty(), "a deferred tile is owed a frame");
        }
        if frame == 2 {
            assert!(
                owed.is_empty(),
                "the third frame rasterises every needed tile"
            );
            // A blank keeps its frame, which says where it would have drawn.
            let visible = |sprite: &&ColorSprite| {
                sprite.frame[0] + 1.0 < viewport.width as f32
                    && sprite.frame[1] + 1.0 < viewport.height as f32
            };
            assert!(
                scene
                    .primitives
                    .color_sprites
                    .iter()
                    .filter(visible)
                    .all(|sprite| sprite.bounds != [0.0; 4]),
                "nothing visible is blank on the third frame"
            );
        }
        fixture.frame();
    }
    assert!(deferred_frames >= 1, "the budget defers some tiles");
    assert!(!first_owed.is_empty());
}

#[test]
fn an_edit_of_one_shape_rasterises_only_its_tiles() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let shapes = quarters(100.0);
    let drawing = Drawing::fitted_shared(Arc::from(shapes.clone()), Affine::IDENTITY);
    let first = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    let viewport = Size::new(3100, 3100);
    for _ in 0..4 {
        fixture.settle(&first, 30.0, viewport, &DamageSet::full());
        fixture.frame();
    }
    // A new revision: the same paths, and the last quarter's square in another colour.
    let mut edited = shapes;
    edited[4] = zgui_svg::Shape {
        fill: Some(zgui_svg::Fill {
            paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(Color::WHITE)),
            rule: peniko::Fill::NonZero,
        }),
        ..edited[4].clone()
    };
    let drawing = Drawing::fitted_shared(Arc::from(edited), Affine::IDENTITY);
    let reused = get(Counter::VectorLayerTilesReused);
    let second = tiles(fixture.ask(2, &drawing, 30.0, [0.0, 0.0]));
    assert_ne!(first.id(), second.id());
    let changed = first
        .sprites()
        .zip(second.sprites())
        .filter(|(a, b)| a.0 != b.0)
        .count();
    assert_eq!(
        get(Counter::VectorLayerTilesReused) - reused,
        (second.len() - changed) as u64
    );
    let before = get(Counter::VectorLayerTilesRasterised);
    for _ in 0..4 {
        fixture.settle(&second, 30.0, viewport, &DamageSet::full());
        fixture.frame();
    }
    assert_eq!(
        get(Counter::VectorLayerTilesRasterised) - before,
        changed as u64,
        "only the tiles the square meets"
    );
    assert!(changed > 0 && changed < second.len() / 2);
}

#[test]
fn evicted_tiles_rasterise_again() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(quarters(100.0)), Affine::IDENTITY);
    let tiled = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    let viewport = Size::new(1024, 1024);
    let mut damage = DamageSet::new();
    damage.absorb(Rect::new(Point::new(0, 0), Size::new(1024, 1024)));
    for _ in 0..4 {
        fixture.settle(&tiled, 30.0, viewport, &damage);
        fixture.frame();
    }
    let held = fixture.cache.bytes();
    assert!(held > 0);
    // The budget's step runs between frames, when nothing is pinned.
    fixture.frame();
    let freed = fixture.cache.evict(&mut fixture.atlas, u64::MAX);
    assert_eq!(freed, held);
    assert_eq!(fixture.cache.bytes(), 0);
    let before = get(Counter::VectorLayerTilesRasterised);
    let (scene, _) = fixture.settle(&tiled, 30.0, viewport, &damage);
    assert!(get(Counter::VectorLayerTilesRasterised) > before);
    assert!(
        scene
            .primitives
            .color_sprites
            .iter()
            .any(|sprite| sprite.bounds != [0.0; 4])
    );
}

#[test]
fn a_source_nothing_draws_is_dropped() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(quarters(100.0)), Affine::IDENTITY);
    let tiled = tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    assert!(fixture.cache.tiles_alive(tiled.id()));
    for _ in 0..70 {
        fixture.frame();
    }
    assert!(!fixture.cache.tiles_alive(tiled.id()));
}
