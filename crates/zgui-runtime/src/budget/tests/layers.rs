//! What the layer budget frees, over a real content cache.

use kurbo::{Affine, BezPath};
use zgui_atlas::AtlasLimits;
use zgui_paint::ContentCache;
use zgui_paint::VectorId;
use zgui_paint::content::vectors::{Drawing, LayerAnswer, LayerRequest, outlines};
use zgui_paint::emit::vector::ShapePaint;

use crate::budget::caches::VectorLayersBudget;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::tests::at;

/// A drawing of one 32 unit square of its own, fitted at one.
fn drawing() -> Drawing {
    let square = BezPath::from_svg("M0 0 L32 0 L32 32 L0 32 Z").expect("a square");
    Drawing::fitted(outlines(&[square], Affine::IDENTITY), Affine::IDENTITY)
}

/// Asks for the layer of `drawing` for `owner`, and reports whether it was rasterised for this.
fn layer(content: &mut ContentCache, owner: u32, drawing: &Drawing) -> bool {
    let before = content.layer_bytes();
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
    assert!(
        matches!(answer, LayerAnswer::Sprite { .. }),
        "expected a layer, got {answer:?}"
    );
    content.layer_bytes() > before
}

#[test]
fn the_layer_budget_evicts_layers_the_frame_did_not_draw_oldest_first() {
    let mut content = ContentCache::new(AtlasLimits::default());
    let drawings = [drawing(), drawing(), drawing()];
    for (owner, drawing) in drawings.iter().enumerate() {
        content.begin_frame();
        assert!(layer(&mut content, owner as u32, drawing));
        content.end_frame();
    }
    let one = 32 * 32 * 4;
    assert_eq!(content.layer_bytes(), 3 * one);
    // The oldest is drawn again this frame.
    content.begin_frame();
    assert!(!layer(&mut content, 0, &drawings[0]), "held from before");

    let mut tracked = Tracked::default();
    let mut budget = VectorLayersBudget::new(&mut content, one, &mut tracked);
    assert_eq!(budget.report().resident, 3 * one);
    assert_eq!(budget.report().pinned, one, "this frame drew one of them");
    assert_eq!(budget.evict(one, at(1)), one);
    assert_eq!(budget.evict(u64::MAX, at(1)), one);
    assert_eq!(content.layer_bytes(), one, "the drawn layer stays");
    assert!(
        !layer(&mut content, 0, &drawings[0]),
        "the layer the frame drew is still held"
    );
}
