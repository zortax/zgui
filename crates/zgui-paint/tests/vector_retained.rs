//! What a canvas keeps between frames when only its fit changes.

mod support;

use std::sync::Arc;

use zgui_color::Color;
use zgui_paint::content::NoVectorMasks;
use zgui_paint::emit::record::Recording;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw_drawing};

use zgui_paint::VectorId;
use zgui_space::{ClipId, SpatialId};

use support::conformance::scene_at;

/// A canvas panned to a new fit records its general shapes with the same paths.
///
/// The fit is the items' placement, so a pan changes no path and no paint, and the compositor
/// draws the second frame from the paths it already holds.
#[test]
fn a_panned_canvas_keeps_its_paths() {
    // A solid paint, because a recording without a compositor has no paint slots.
    let red = zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(Color::srgb_u8(255, 0, 0, 255)));
    // Triangles, which no recognising route takes.
    let shapes: Vec<zgui_svg::Shape> = (0..3)
        .map(|index| {
            let x = f64::from(index) * 20.0;
            let mut path = kurbo::BezPath::new();
            path.move_to((x, 0.0));
            path.line_to((x + 16.0, 2.0));
            path.line_to((x + 5.0, 14.0));
            path.close_path();
            zgui_svg::Shape {
                path: Arc::new(path),
                fill: Some(zgui_svg::Fill {
                    paint: red.clone(),
                    rule: peniko::Fill::NonZero,
                }),
                stroke: None,
                clips: Vec::new(),
            }
        })
        .collect();
    let shapes: Arc<[zgui_svg::Shape]> = Arc::from(shapes);
    let recorded = |fit: kurbo::Affine| {
        let mut scene = scene_at(128);
        let mut recording = Recording::new();
        let mut rec = recording.recorder(&mut scene, SpatialId::VIEWPORT, ClipId::ROOT);
        let drawing = zgui_paint::Drawing::fitted_shared(Arc::clone(&shapes), fit);
        let pushed = draw_drawing(
            &mut rec,
            VectorId(1),
            &drawing,
            ShapePaint {
                fill: Color::WHITE,
                stroke: None,
                stroke_width: 1.0,
            },
            &NoVectorMasks,
            VectorPlacement {
                clip: ClipId::ROOT,
                transform: SpatialId::VIEWPORT,
                scale: 1.0,
            },
        );
        assert_eq!(pushed, 3);
        recording
    };

    let before = recorded(kurbo::Affine::translate((4.0, 8.0)) * kurbo::Affine::scale(1.5));
    let after = recorded(kurbo::Affine::translate((-9.5, 30.25)) * kurbo::Affine::scale(1.5));
    assert_eq!(before.paths().len(), 3);
    for (before, after) in before.paths().iter().zip(after.paths()) {
        assert!(
            Arc::ptr_eq(&before.path, &after.path),
            "the pan records a general shape's path again"
        );
        assert_eq!((before.brush, before.fill), (after.brush, after.fill));
        assert_ne!(before.placement, after.placement, "the fit moves the path");
    }
}
