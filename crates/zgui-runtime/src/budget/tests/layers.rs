//! What the layer budget frees, over a real content cache.

use zgui_atlas::AtlasLimits;
use zgui_paint::content::vectors::{Drawing, LayerAnswer, LayerRequest, outlines};
use zgui_paint::emit::vector::ShapePaint;
use zgui_paint::walk::replay::hold::ResourceOwner as _;
use zgui_paint::{ContentCache, Painter};
use zgui_scene::kurbo::{Affine, BezPath};
use zgui_scene::{Scene, VectorId};

use crate::budget::caches::VectorLayersBudget;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::tests::at;

/// A drawing of one 32 unit square of its own, fitted at one.
fn drawing() -> Drawing {
    let square = BezPath::from_svg("M0 0 L32 0 L32 32 L0 32 Z").expect("a square");
    Drawing::fitted(outlines(&[square], Affine::IDENTITY), Affine::IDENTITY)
}

/// Rasterises `drawing` for `owner` in a frame of its own, and answers the tile's key.
fn layer(content: &mut ContentCache, owner: u32, drawing: &Drawing) -> zgui_atlas::AtlasKey {
    content.begin_frame();
    let answer = content.layer(LayerRequest {
        owner: VectorId(owner),
        revision: u64::from(owner),
        drawing,
        paint: ShapePaint {
            fill: zgui_color::Color::BLACK,
            stroke: None,
            stroke_width: 1.0,
        },
        spatial: Some(zgui_geom::Affine2::IDENTITY),
    });
    content.end_frame();
    match answer {
        LayerAnswer::Sprite { key, .. } => key,
        other => panic!("expected a layer, got {other:?}"),
    }
}

#[test]
fn the_layer_budget_evicts_unheld_layers_oldest_first() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let drawings = [drawing(), drawing(), drawing()];
    let keys: Vec<_> = drawings
        .iter()
        .enumerate()
        .map(|(owner, drawing)| layer(&mut content, owner as u32, drawing))
        .collect();
    let one = 32 * 32 * 4;
    assert_eq!(content.layer_bytes(), 3 * one);
    // The oldest is held, as a record drawing it would hold it.
    content.tile_owner().retain(keys[0]);
    content.begin_frame();

    let mut painter = Painter::new();
    let mut scene = Scene::new();
    let mut tracked = Tracked::default();
    let mut budget =
        VectorLayersBudget::new(&mut painter, &mut scene, &mut content, one, &mut tracked);
    assert_eq!(budget.report().resident, 3 * one);
    assert_eq!(budget.report().pinned, 0, "this frame drew none of them");
    assert_eq!(budget.evict(one, at(1)), one);
    assert!(
        !content.atlas().contains(keys[1]),
        "the oldest unheld layer goes first"
    );
    assert!(content.atlas().contains(keys[2]));
    let mut budget =
        VectorLayersBudget::new(&mut painter, &mut scene, &mut content, one, &mut tracked);
    assert_eq!(budget.evict(u64::MAX, at(1)), one);
    assert!(content.atlas().contains(keys[0]), "a held layer stays");
    assert!(!content.atlas().contains(keys[2]));
    assert_eq!(content.layer_bytes(), one);
}
