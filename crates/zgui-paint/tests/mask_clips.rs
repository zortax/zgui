//! Clip chains with a mask link, mirrored as mask clips over the image of the link's texture.

mod support;

use compositor_types::{ImageDesc, ImageFormat};
use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_paint::emit::record::{ImageKey, ImagePixels, filled};
use zgui_scene::{ClipId, ClipLink, MaskSource, Scene, SpatialId};

use support::chunk::Lone;

/// A 16 by 8 tile at `(4, 2)` of the first alpha texture.
fn tile() -> AtlasTile {
    AtlasTile {
        texture: TextureId::new(TextureKind::Mono, 0),
        tile: TileId(3),
        bounds: Rect::new(Point::new(4, 2), Size::new(16, 8)),
    }
}

/// A rectangle of device pixels.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// Records a red quad through a clip whose mask link samples `tile()`, with the image of the
/// tile's texture recorded first when `texture` is true, and returns the quad's line.
fn masked(texture: bool) -> String {
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(256, 256));
    let chain = scene.clips.push(
        ClipId::ROOT,
        ClipLink::Mask {
            tile: tile(),
            transform: SpatialId::VIEWPORT,
            source: MaskSource::Raster,
        },
    );
    let texels = vec![255u8; 32 * 16];
    let mut lone = Lone::record(&mut scene, (SpatialId::VIEWPORT, ClipId::ROOT), |rec| {
        if texture {
            let pixels = ImagePixels {
                desc: ImageDesc {
                    width: 32,
                    height: 16,
                    format: ImageFormat::A8,
                },
                revision: 1,
                texels: Some(&texels),
            };
            rec.image_slot(ImageKey::of_texture(tile().texture), Some(pixels))
                .expect("the image is created");
        }
        let red = rec.color(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
        let mut quad = filled(rect(0.0, 0.0, 64.0, 64.0), [0.0; 8], red);
        quad.clip = rec.clip(chain);
        rec.quad(quad);
    });
    let drawing = lone.redraw();
    drawing.quads()[0].text().to_owned()
}

#[test]
fn a_mask_link_clips_through_the_image_of_its_texture() {
    let quad = masked(true);
    assert!(
        quad.contains("clip=[mask rect(0, 0, 16, 8)"),
        "the mask covers the tile's extent where its transform puts it: {quad}"
    );
    assert!(quad.contains("mode=alpha"), "{quad}");
}

#[test]
fn a_mask_link_whose_texture_has_no_image_clips_nothing() {
    let quad = masked(false);
    assert!(!quad.contains("clip="), "{quad}");
}
