//! The one call that turns a renderer into one that can draw a path.
//!
//! A renderer without a rasteriser is not a broken renderer. It configures, it draws, it plans the
//! vector passes of a display list and it counts them; what it does not do is put anything in them.
//! That failure has no error, no warning and no wrong colour attached to it — only an empty
//! rectangle where a drawing should be — so the wiring is asserted here rather than left to be
//! noticed on a screen.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, wgpu};
use zgui_scene::ClipId;

use support::{opaque, path, quad, rect};

/// The extent this fixture draws at.
const SIDE: i32 = 64;

/// A renderer over a stand-in surface, with no rasteriser attached yet.
fn bare() -> Option<zgui_render_wgpu::WgpuRenderer> {
    let target = RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0));
    match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
        Ok(renderer) => Some(renderer),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            None
        }
    }
}

/// A black frame with one white square drawn as a path in the middle of it.
fn square() -> zgui_scene::Scene {
    let mut scene = support::scene_at(SIDE);
    quad(&mut scene, rect(0.0, 0.0, 64.0, 64.0), opaque(0, 0, 0));
    support::vector(
        &mut scene,
        0,
        path(rect(16.0, 16.0, 32.0, 32.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// Attaching leaves a renderer that draws the path a display list asks for.
///
/// Both halves matter. The first is the state a window was found in — a renderer that reports a
/// vector pass and draws nothing — and the second is that the call fixes it, on this device, in
/// pixels.
#[test]
fn a_renderer_draws_no_path_until_it_is_attached_and_draws_one_afterwards() {
    let _guard = support::device_lock();
    let Some(mut renderer) = bare() else {
        return;
    };

    assert!(
        !renderer.has_vector_raster(),
        "a renderer arrives with no rasteriser, which is what makes attaching one necessary"
    );
    let scene = square();
    let outcome = renderer.draw(&scene, &DamageSet::full());
    assert_eq!(
        outcome.stats().expect("presented").vector_passes,
        1,
        "the display list planned the pass whether or not anything could run it"
    );
    let blank = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        blank.rgba(32, 32),
        [0, 0, 0, 255],
        "with no rasteriser the middle of the square is the background, which is the blank icon"
    );

    zgui_render_vector_vello::attach(&mut renderer, Size::new(SIDE, SIDE));
    assert!(renderer.has_vector_raster());

    let scene = square();
    renderer.draw(&scene, &DamageSet::full());
    let drawn = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        drawn.rgba(32, 32),
        [255, 255, 255, 255],
        "the same display list through the same renderer, now attached, still drew nothing"
    );
    assert_eq!(
        drawn.rgba(4, 4),
        [0, 0, 0, 255],
        "and it drew the path rather than filling the frame"
    );
}
