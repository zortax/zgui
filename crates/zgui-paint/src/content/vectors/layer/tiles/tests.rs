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

/// Ten by ten low zigzags over 0..100 in both axes, each of 2000 lines in new paths.
///
/// Many segments per tile and few texels, so the frame's budget admits few tiles and each
/// rasterises quickly.
fn zigzags() -> Vec<zgui_svg::Shape> {
    let mut shapes = Vec::new();
    for row in 0..10 {
        for column in 0..10 {
            let mut path = kurbo::BezPath::new();
            let (x, y) = (f64::from(column) * 10.0, f64::from(row) * 10.0);
            path.move_to((x, y));
            for step in 0..2000 {
                let t = f64::from(step) / 2000.0;
                path.line_to((x + 10.0 * t, y + if step % 2 == 0 { 0.0 } else { 1.0 }));
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
    let shapes = zigzags();
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

#[test]
fn an_edit_that_moves_the_bounds_keeps_every_raster_the_size_of_its_slot() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let colour = Color::srgb(0.2, 0.6, 0.9, 1.0);
    let top = solid(kurbo::Rect::new(0.0, 0.0, 100.0, 50.0), colour);
    let bottom = solid(kurbo::Rect::new(0.0, 60.0, 90.0, 100.0), colour);
    let drawing = Drawing::fitted_shared(Arc::from(vec![top.clone(), bottom]), Affine::IDENTITY);
    tiles(fixture.ask(1, &drawing, 30.0, [0.0, 0.0]));
    fixture.frame();
    // The bottom shape grows past the top one, so the right edge of the drawing moves in the
    // rows only the top shape meets.
    let grown = solid(kurbo::Rect::new(0.0, 60.0, 101.0, 100.0), colour);
    let drawing = Drawing::fitted_shared(Arc::from(vec![top, grown]), Affine::IDENTITY);
    let second = tiles(fixture.ask(2, &drawing, 30.0, [0.0, 0.0]));
    let rasters = &fixture.cache.tiles.rasters;
    for slot in &second.0.slots {
        let raster = &rasters[&slot.handle];
        let made = &raster.source.slots[raster.slot as usize];
        assert_eq!(made.texels, slot.texels, "the raster of {:?}", slot.texels);
    }
    assert!(
        second
            .0
            .slots
            .iter()
            .any(|slot| slot.texels[2] == 3030 && slot.texels[1] == 0),
        "the top row reaches the new edge"
    );
}

/// The blank frames in a row of each visible sprite of `scene`, by where it is, after this frame.
fn count_blanks(
    blanks: &mut rustc_hash::FxHashMap<[i32; 2], u32>,
    scene: &Scene,
    viewport: Size<i32, Device>,
) {
    for sprite in &scene.primitives.color_sprites {
        let at = [sprite.frame[0] as i32, sprite.frame[1] as i32];
        if at[0] >= viewport.width || at[1] >= viewport.height {
            continue;
        }
        let run = blanks.entry(at).or_default();
        *run = if sprite.bounds == [0.0; 4] {
            *run + 1
        } else {
            0
        };
    }
}

#[test]
fn a_drawing_that_changes_every_frame_draws_each_visible_tile_by_its_third_frame() {
    let _turn = zgui_profile::counter::exclusive();
    for ready in [false, true] {
        let mut fixture = Fixture::new();
        fixture.cache.set_raster_ready(ready);
        let viewport = Size::new(1024, 1024);
        let mut blanks = rustc_hash::FxHashMap::default();
        let mut deferred = 0;
        for frame in 0..5_u64 {
            // New paths on every frame, so no tile finds the raster of the frame before.
            let drawing = Drawing::fitted_shared(Arc::from(zigzags()), Affine::IDENTITY);
            let answer = fixture.ask(frame + 1, &drawing, 25.0, [0.0, 0.0]);
            if ready && frame >= 3 {
                assert_eq!(
                    answer,
                    LayerAnswer::Items(super::super::LayerFallback::Demoted),
                    "a drawing that churns leaves the tiles once the general route is built"
                );
                fixture.frame();
                continue;
            }
            let tiled = tiles(answer);
            let before = get(Counter::VectorLayerTilesDeferred);
            let (scene, _) = fixture.settle(&tiled, 25.0, viewport, &DamageSet::full());
            deferred += get(Counter::VectorLayerTilesDeferred) - before;
            count_blanks(&mut blanks, &scene, viewport);
            assert!(
                blanks.values().all(|run| *run <= 2),
                "ready {ready}, frame {frame}: a visible tile is blank on three frames: {blanks:?}"
            );
            fixture.frame();
        }
        assert!(deferred > 0, "the budget defers some tiles");
    }
}

#[test]
fn a_tile_deferred_on_earlier_frames_waits_for_the_budget_again() {
    let _turn = zgui_profile::counter::exclusive();
    let mut fixture = Fixture::new();
    let drawing = Drawing::fitted_shared(Arc::from(zigzags()), Affine::IDENTITY);
    let tiled = tiles(fixture.ask(1, &drawing, 25.0, [0.0, 0.0]));
    let viewport = Size::new(2048, 1536);
    for _ in 0..2 {
        fixture.settle(&tiled, 25.0, viewport, &DamageSet::full());
        fixture.frame();
    }
    // The frames between need nothing.
    for _ in 0..3 {
        fixture.settle(&tiled, 25.0, viewport, &DamageSet::new());
        fixture.frame();
    }
    let before = get(Counter::VectorLayerTilesRasterised);
    let (_, owed) = fixture.settle(&tiled, 25.0, viewport, &DamageSet::full());
    assert!(
        !owed.is_empty(),
        "the tiles deferred twice before are not forced past the budget"
    );
    assert!(get(Counter::VectorLayerTilesRasterised) - before <= 2);
}

/// A 6000 by 4000 illustration like the bench's huge one: a ground, polygons in solid colours,
/// ramps and clips, and strokes.
fn illustration() -> String {
    let mut seed = 0x00C0_FFEE_u64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 11) as f64 / (1_u64 << 53) as f64
    };
    let palette = [
        "#4f8cff", "#ef476f", "#06d6a0", "#ffd166", "#8338ec", "#118ab2",
    ];
    let (width, height) = (6000.0, 4000.0);
    let mut defs = String::new();
    let mut body =
        format!(r##"<rect x="0" y="0" width="{width}" height="{height}" fill="#1b1e26"/>"##);
    for index in 0..480 {
        let (cx, cy) = (next() * width, next() * height);
        let radius = 40.0 + next() * 260.0;
        let corners = 3 + (next() * 5.0) as usize;
        let mut path = String::new();
        for corner in 0..corners {
            let angle = std::f64::consts::TAU * corner as f64 / corners as f64 + next();
            let reach = radius * (0.6 + 0.4 * next());
            let (x, y) = (cx + reach * angle.cos(), cy + reach * angle.sin());
            path.push_str(&format!(
                "{}{x:.1} {y:.1} ",
                if corner == 0 { "M" } else { "L" }
            ));
        }
        path.push('Z');
        let (first, second) = (palette[index % 6], palette[(index * 3 + 1) % 6]);
        let fill = if index % 3 == 0 {
            defs.push_str(&format!(
                r##"<linearGradient id="l{index}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{first}"/><stop offset="1" stop-color="{second}"/></linearGradient>"##
            ));
            format!("url(#l{index})")
        } else {
            first.to_owned()
        };
        if index % 8 == 5 {
            defs.push_str(&format!(
                r##"<clipPath id="c{index}"><circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}"/></clipPath>"##,
                radius * 0.7
            ));
            body.push_str(&format!(
                r##"<g clip-path="url(#c{index})"><path d="{path}" fill="{fill}"/></g>"##
            ));
        } else {
            body.push_str(&format!(r##"<path d="{path}" fill="{fill}"/>"##));
        }
    }
    for index in 0..160 {
        let (mut x, mut y) = (next() * width, next() * height);
        let mut path = format!("M{x:.1} {y:.1}");
        for _ in 0..12 {
            x += (next() - 0.5) * 300.0;
            y += (next() - 0.5) * 300.0;
            path.push_str(&format!(" L{x:.1} {y:.1}"));
        }
        let colour = palette[(index + 3) % 6];
        body.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="{colour}" stroke-width="6"/>"##
        ));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}"><defs>{defs}</defs>{body}</svg>"##
    )
}

/// Measures the painter on every tile of a large illustration and compares each tile's estimate
/// with its time.
///
/// A measurement, so it runs only when `ZGUI_CALIBRATE` is set:
///
/// ```text
/// ZGUI_CALIBRATE=1 cargo test -p zgui-paint --release calibrate_the_tile_cost_model -- --nocapture
/// ```
#[test]
fn calibrate_the_tile_cost_model() {
    use crate::content::vectors::cpu::{LayerJob, VectorPainter as _};

    if std::env::var_os("ZGUI_CALIBRATE").is_none() {
        eprintln!(
            "calibrate_the_tile_cost_model: set ZGUI_CALIBRATE to measure the tile estimates"
        );
        return;
    }
    let document = zgui_svg::parse(&illustration()).expect("the illustration parses");
    let drawing = Drawing::fitted_shared(Arc::from(document.shapes()), Affine::IDENTITY);
    let mut fixture = Fixture::new();
    let tiled = tiles(fixture.ask(1, &drawing, 1.0, [0.0, 0.0]));
    let source = &tiled.0;
    let mut rows = Vec::new();
    for slot in &source.slots {
        let [x0, y0, x1, y1] = slot.texels;
        let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut texels = vec![0; width as usize * height as usize * 4];
        let job = LayerJob {
            shapes: &source.shapes,
            only: Some(&slot.shapes),
            paint: source.paint,
            map: Affine::translate((-f64::from(x0), -f64::from(y0))) * source.linear,
            stroke_scale: source.stroke_scale,
            inherited_stroke: source.inherited_stroke,
            width,
            height,
        };
        fixture.cache.painter.paint(&job, &mut texels);
        let runs = 5;
        let start = std::time::Instant::now();
        for _ in 0..runs {
            fixture.cache.painter.paint(&job, &mut texels);
        }
        let us = start.elapsed().as_secs_f64() * 1.0e6 / f64::from(runs);
        rows.push((slot.cost, us));
    }
    // Least squares over us = a·pixels + b·segments + c·shapes.
    let terms = |cost: &super::TileCost| [cost.pixels, cost.segments as f64, cost.shapes as f64];
    let mut normal = [[0.0_f64; 4]; 3];
    for (cost, us) in &rows {
        let terms = terms(cost);
        for row in 0..3 {
            for column in 0..3 {
                normal[row][column] += terms[row] * terms[column];
            }
            normal[row][3] += terms[row] * us;
        }
    }
    for pivot in 0..3 {
        for row in 0..3 {
            if row != pivot {
                let factor = normal[row][pivot] / normal[pivot][pivot];
                let above = normal[pivot];
                for (value, from) in normal[row].iter_mut().zip(above) {
                    *value -= factor * from;
                }
            }
        }
    }
    let fit: Vec<f64> = (0..3)
        .map(|row| normal[row][3] / normal[row][row])
        .collect();
    println!(
        "TILE_US_PER_PIXEL = {:.5}, TILE_US_PER_SEGMENT = {:.3}, TILE_US_PER_SHAPE = {:.1}",
        fit[0], fit[1], fit[2]
    );
    let measured: f64 = rows.iter().map(|(_, us)| us).sum();
    let estimated: f64 = rows.iter().map(|(cost, _)| cost.us()).sum();
    println!(
        "{} tiles: {measured:.0} us measured, {estimated:.0} us estimated, ratio {:.3}",
        rows.len(),
        estimated / measured
    );
}
