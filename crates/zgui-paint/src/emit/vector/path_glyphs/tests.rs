//! How the split cache keeps what emission splits.

use core::cell::{RefCell, RefMut};
use std::sync::Arc;

use zgui_atlas::{AtlasKey, TextureKind};
use zgui_scene::kurbo::BezPath;
use zgui_scene::peniko;

use super::{Part, lower, split_of};
use crate::content::vectors::{
    GlyphSheets, Splits, VectorMask, VectorMaskRequest, VectorMaskSource, VectorMaskStyle,
};

/// The identity map.
const IDENTITY: [f64; 4] = [1.0, 0.0, 0.0, 1.0];

/// A source that keeps splits and nothing else.
#[derive(Default)]
struct Source(RefCell<Splits>);

impl VectorMaskSource for Source {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn glyph_splits(&self) -> Option<RefMut<'_, Splits>> {
        Some(self.0.borrow_mut())
    }
}

/// Eight triangles 20 units apart.
fn triangles() -> Arc<BezPath> {
    let mut path = BezPath::new();
    for index in 0..8 {
        let x = 20.0 * f64::from(index);
        path.move_to((x, 0.0));
        path.line_to((x + 4.0, 6.0));
        path.line_to((x - 4.0, 6.0));
        path.close_path();
    }
    Arc::new(path)
}

/// Splits `path` and lowers its fill, as emission does in one frame.
fn emit(source: &Source, path: &Arc<BezPath>) {
    let found = split_of(path, IDENTITY, source).expect("one outline");
    let sheets = GlyphSheets {
        texture: 0,
        table: vec![[0; 4]; 16],
        reach: vec![[-5, -1, 6, 8]],
        keys: vec![AtlasKey::new(1, TextureKind::Mono)],
    };
    let part = Part {
        index: 0,
        style: VectorMaskStyle::Fill(peniko::Fill::NonZero),
        scale: 1.0,
        rule: Some(peniko::Fill::NonZero),
    };
    let place = |x: f64, y: f64| [x, y];
    lower(path, IDENTITY, &found, &sheets, &part, &place, source).expect("a payload");
}

/// Runs one frame of `draw`.
fn frame(source: &Source, draw: impl FnOnce()) {
    source.0.borrow_mut().begin_frame();
    draw();
    source.0.borrow_mut().end_frame();
}

#[test]
fn an_entry_no_later_frame_finds_is_dropped_after_one_frame() {
    let source = Source::default();
    let path = triangles();
    frame(&source, || {
        emit(&source, &path);
        let mut splits = source.0.borrow_mut();
        let entry = splits.held(&path, IDENTITY).expect("kept");
        assert!(
            entry.payloads[0].is_some(),
            "the payload is kept with the split"
        );
    });
    assert_eq!(source.0.borrow().len(), 1, "kept for the next frame");
    frame(&source, || {});
    assert_eq!(
        source.0.borrow().len(),
        0,
        "the frame that split the path does not prove its entry"
    );

    // Found in the next frame, the entry is proven and outlives a frame without the path.
    frame(&source, || emit(&source, &path));
    frame(&source, || emit(&source, &path));
    frame(&source, || {});
    assert_eq!(source.0.borrow().len(), 1);
}

#[test]
fn paths_split_once_each_stop_the_cache_keeping() {
    let source = Source::default();
    // A drawing placed again every frame: each frame splits new allocations and finds none.
    let mut drawn = Vec::new();
    for _ in 0..2 {
        frame(&source, || {
            for _ in 0..16 {
                let path = triangles();
                emit(&source, &path);
                drawn.push(path);
            }
        });
    }
    assert_eq!(
        source.0.borrow().len(),
        0,
        "a frame after one that found nothing keeps nothing new"
    );
}
