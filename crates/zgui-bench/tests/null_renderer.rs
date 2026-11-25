//! What the measuring renderer reports to the compositor.

#![forbid(unsafe_code)]

use iceblink::{BufferAge, Frame, RenderTarget, Renderer};
use zgui_bench::renderers::NullRenderer;

fn target(size: [u32; 2], scale: f32) -> RenderTarget {
    RenderTarget {
        size,
        scale,
        opaque: true,
    }
}

#[test]
fn a_resize_keeps_the_picture() {
    let mut renderer = NullRenderer::new(target([800, 600], 1.0));
    renderer.draw(&Frame::default());

    renderer.configure(target([808, 600], 1.0));

    assert_eq!(renderer.target_age(), BufferAge::Kept);
}

#[test]
fn a_scale_change_drops_the_picture() {
    let mut renderer = NullRenderer::new(target([800, 600], 1.0));
    renderer.draw(&Frame::default());

    renderer.configure(target([800, 600], 2.0));

    assert_eq!(renderer.target_age(), BufferAge::Unknown);
}

#[test]
fn lowered_chunks_stay_resident() {
    let renderer = NullRenderer::new(target([800, 600], 1.0));

    assert!(renderer.capabilities().resident_chunks);
}
