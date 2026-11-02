//! A clip kept in a scrolled content space draws what the same clip put on the device draws.
//!
//! A chain whose innermost links are measured in a coordinate system that only translates keeps
//! those links in that system's coordinates, and the shader moves the device point back by the
//! system's translation before it tests them. A scroll then writes one matrix and no clip. These
//! are the pictures that say the shader reads the kept part where the device form puts it: one
//! scrolled through a content space over several frames, one drawn from scratch in device
//! coordinates at the place it scrolled to, and no channel of no pixel between them.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Corners, Device, DevicePx, Matrix4, Rect, Size, Vec2};
use zgui_render_wgpu::Pixels;
use zgui_scene::{
    Anchoring, ClipLink, CornerShape, Paint, PropertyOwner, Quad, Scene, SpatialId, SpatialNode,
};

use support::{SIDE, opaque, present, rect, renderer_pair};

/// The scroll offsets the content space passes through. The last is the one compared.
const OFFSETS: [f32; 3] = [10.0, 31.0, 47.0];

/// The rectangle of the row clip, in content coordinates.
const ROW: (f32, f32, f32, f32) = (20.0, 60.0, 80.0, 50.0);

/// `rect` moved up by `by`.
fn up(rect: (f32, f32, f32, f32), by: f32) -> Rect<DevicePx, Device> {
    support::rect(rect.0, rect.1 - by, rect.2, rect.3)
}

/// A rounded link of radius `radius`, measured in `space`.
fn rounded(rect: Rect<DevicePx, Device>, radius: f32, space: SpatialId) -> ClipLink {
    ClipLink::shaped(
        rect,
        Corners::uniform(Vec2::splat(DevicePx(radius))),
        CornerShape::ROUND,
        space,
    )
}

/// Draws one frame: a rounded port on the device, and inside it a rounded row and the content it
/// clips.
///
/// With `content`, the row and its content are measured in that space; otherwise they are moved
/// up by `offset` and measured on the device.
fn draw(scene: &mut Scene, content: Option<SpatialId>, offset: f32) {
    scene.begin_frame(Size::new(SIDE, SIDE));
    let viewport = scene.spatial.viewport();
    let (space, by) = match content {
        Some(space) => (space, 0.0),
        None => (viewport, offset),
    };
    let white = scene.paints.add(Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let port = scene
        .clips
        .only(rounded(rect(8.0, 8.0, 112.0, 112.0), 16.0, viewport));
    let row = scene.clips.push(port, rounded(up(ROW, by), 10.0, space));
    let blue = scene.paints.add(Paint::Solid(opaque(40, 90, 230)));
    let red = scene.paints.add(Paint::Solid(opaque(220, 40, 30)));
    // Content under the port alone, and content under the row; both scroll.
    let under_port = Quad::filled(up((0.0, 0.0, 128.0, 300.0), by), blue).clipped(port);
    let under_row = Quad::filled(up((0.0, 0.0, 128.0, 300.0), by), red).clipped(row);
    scene.push_quad(under_port.transformed(space));
    scene.push_quad(under_row.transformed(space));
    scene.finish(&DamageSet::full());
}

/// Whether `picture` shows red at a point, which is inside the row.
fn red_at(picture: &Pixels, x: i32, y: i32) -> bool {
    let [r, g, b, _] = picture.rgba(x, y);
    r > 200 && g < 80 && b < 80
}

#[test]
fn a_clip_kept_in_a_scrolled_content_space_draws_its_device_form() {
    let Some((mut kept_renderer, mut reference_renderer)) = renderer_pair() else {
        return;
    };
    let mut scrolled = Scene::new();
    let viewport = scrolled.spatial.viewport();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    let node = |offset: f32| SpatialNode {
        parent: Some(viewport),
        local: Matrix4::translation(0.0, -offset, 0.0),
        anchoring: Anchoring::Content,
    };
    let content = scrolled
        .spatial
        .establish(owner.content(), node(OFFSETS[0]));
    let mut pictures = Vec::new();
    for offset in OFFSETS {
        scrolled
            .spatial
            .place(content, Matrix4::translation(0.0, -offset, 0.0));
        draw(&mut scrolled, Some(content), offset);
        pictures.push(present(&mut kept_renderer, &scrolled));
    }

    let mut repainted = Scene::new();
    draw(&mut repainted, None, OFFSETS[2]);
    let reference = present(&mut reference_renderer, &repainted);

    // Red shows inside the row and not above or below it, and the row moves between frames, so
    // a shader that reads the kept part where it was interned draws red where the reference
    // does not.
    let (row_top, row_bottom) = (ROW.1 - OFFSETS[2], ROW.1 + ROW.3 - OFFSETS[2]);
    assert!(red_at(&reference, 60, (row_top + 4.0) as i32));
    assert!(!red_at(&reference, 60, (row_top - 4.0) as i32));
    assert!(!red_at(&reference, 60, (row_bottom + 4.0) as i32));
    assert_ne!(
        pictures[1].max_difference(&reference),
        0,
        "the fixture moves the row between the frames compared"
    );
    let last = pictures.last().expect("one picture per offset");
    assert_eq!(
        last.max_difference(&reference),
        0,
        "a clip kept in a scrolled content space drew differently from its device form"
    );
}
