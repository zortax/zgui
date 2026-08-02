//! What is asked of the device for each frame the application draws.
//!
//! One frame of the application becomes several draws here, because the questions are about how a
//! frame *depends on what was on the target before it*. A partial repaint that quietly redrew
//! everything and a partial repaint that redrew its rectangle are the same picture, so the only way
//! to tell them apart is to put something distinguishable outside the rectangle first and look for
//! it afterwards.

use zgui::bits::DamageSet;
use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::render::Renderer as _;
use zgui::scene::{PaintRef, Quad, Scene};
use zgui_color::Color;
use zgui_render_wgpu::{Pixels, WgpuRenderer};

/// What one of the application's frames produced on the device.
pub struct Recorded {
    /// The frame drawn over the whole target.
    pub full: Pixels,
    /// The same frame drawn with only the drawing's own rectangle in the damage set, over a target
    /// holding a colour that appears nowhere in the frame.
    ///
    /// Nothing when the frame had no drawing in it to scissor to.
    pub scissored: Option<Pixels>,
    /// The same again, over a target holding the page's own background rather than the marker, so
    /// that it is comparable to [`Recorded::full`] pixel for pixel.
    pub replayed: Option<Pixels>,
    /// The rectangle the display list says the drawing's ink covers, in device pixels.
    pub declared: Option<Rect<i32, Device>>,
    /// How many vector passes the frame reported.
    pub vector_passes: u32,
    /// How many vector items the display list held.
    pub items: usize,
}

/// The colour a target is filled with before a scissored repaint, which appears in no frame.
pub const MARKER: [u8; 4] = [255, 0, 255, 255];

/// Draws `scene` on `renderer` every way the assertions need it, and reads each one back.
///
/// The target is left holding the frame drawn over the whole of it, so the application's own
/// presentation is the one it asked for rather than whichever experiment ran last.
pub fn record(renderer: &mut WgpuRenderer, scene: &Scene, background: [u8; 4]) -> Recorded {
    let size = renderer.target().expect("configured").size;
    let outcome = renderer.draw(scene, &DamageSet::full());
    let vector_passes = outcome.stats().map_or(0, |stats| stats.vector_passes);
    let full = read(renderer);

    let declared = ink(scene);
    let items = scene.primitives.vectors.len();
    let Some(rectangle) = declared else {
        return Recorded {
            full,
            scissored: None,
            replayed: None,
            declared: None,
            vector_passes,
            items,
        };
    };

    let mut damage = DamageSet::new();
    damage.absorb(rectangle);

    renderer.draw(&flat(size, MARKER), &DamageSet::full());
    renderer.draw(scene, &damage);
    let scissored = read(renderer);

    renderer.draw(&flat(size, background), &DamageSet::full());
    renderer.draw(scene, &damage);
    let replayed = read(renderer);

    renderer.draw(scene, &DamageSet::full());
    Recorded {
        full,
        scissored: Some(scissored),
        replayed: Some(replayed),
        declared: Some(rectangle),
        vector_passes,
        items,
    }
}

/// Reads the stand-in surface back.
fn read(renderer: &WgpuRenderer) -> Pixels {
    renderer
        .read_presented()
        .expect("these fixtures draw to a texture, which can be read back")
}

/// The rectangle every vector item's ink covers, rounded outwards to whole pixels.
///
/// Taken from the display list rather than from the layout tree because it is the display list's
/// own claim about where the ink goes, and the whole question here is whether the device put the
/// ink where the display list said it would.
fn ink(scene: &Scene) -> Option<Rect<i32, Device>> {
    let mut bounds: Option<Rect<DevicePx, Device>> = None;
    for item in &scene.primitives.vectors {
        bounds = Some(match bounds {
            None => item.ink,
            Some(so_far) => so_far.union(item.ink),
        });
    }
    let bounds = bounds?;
    let left = bounds.left().0.floor() as i32;
    let top = bounds.top().0.floor() as i32;
    let right = bounds.right().0.ceil() as i32;
    let bottom = bounds.bottom().0.ceil() as i32;
    Some(Rect::new(
        Point::new(left, top),
        Size::new(right - left, bottom - top),
    ))
}

/// A display list that is one colour over the whole of `size` and nothing else.
fn flat(size: Size<i32, Device>, color: [u8; 4]) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(size);
    let solid = Color::srgb_u8(color[0], color[1], color[2], color[3]);
    let paint = PaintRef::solid(scene.paints.solid(solid));
    scene.push_quad(Quad::filled(
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(size.width as f32), DevicePx(size.height as f32)),
        ),
        paint,
    ));
    scene.finish(&DamageSet::full());
    scene
}
