//! Scenes the transcript and capture tests are written over.

use std::sync::Arc;

use smallvec::smallvec;
use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
use zgui_bits::DamageSet;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Affine2, Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_scene::kurbo::{self, Shape};
use zgui_scene::peniko::{BlendMode, Compose, Mix};
use zgui_scene::prim::{BorderStyle, DecorationStyle};
use zgui_scene::{
    BackdropFilter, ClipLink, ColorSprite, Decoration, ExternalQuad, ExternalTextureId, Filter,
    GradientKind, GroupBoundary, MaskSource, MonoSprite, Paint, PaintRef, Quad, Scene, Shadow,
    SubpixelSprite, VectorId, VectorItem,
};

/// A device rectangle.
pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// An atlas allocation in one of the three pools.
pub(crate) fn tile(kind: TextureKind, index: u32) -> AtlasTile {
    AtlasTile {
        texture: TextureId::new(kind, 0),
        tile: TileId(index),
        bounds: Rect::new(Point::new(index as i32 * 8, 0), Size::new(8, 16)),
    }
}

/// A scene holding at least one of every primitive kind, so that a transcript rendering that
/// dropped a kind, a field or an ordering would be visible.
///
/// It is deliberately not a realistic document. What it is, is *exhaustive*: a stability test over a
/// scene with one quad in it would be stable and would prove nothing about the other ten kinds.
pub(crate) fn kitchen_sink() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));

    let flat = PaintRef::solid(scene.paints.solid(Color::srgb(0.2, 0.4, 0.6, 1.0)));
    let ramp = scene.paints.add(Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(256.0), DevicePx(0.0)),
        },
        stops: smallvec![
            GradientStop::new(0.0, Color::srgb(1.0, 0.0, 0.0, 1.0)),
            GradientStop::new(1.0, Color::new(ColorSpace::Oklch, [0.7, 0.1, 320.0], 1.0)),
        ],
        space: ColorSpace::Oklab,
        hue: HueInterpolation::Shorter,
        repeating: false,
    });

    let card = rect(8.0, 8.0, 240.0, 96.0);
    let rounded = scene
        .clips
        .only(ClipLink::rounded(card, Vec2::splat(DevicePx(6.0))));
    let masked = scene.clips.push(
        rounded,
        ClipLink::Mask {
            tile: tile(TextureKind::Mono, 3),
            transform: zgui_scene::SpatialId::VIEWPORT,
            source: MaskSource::Path,
        },
    );
    let spun = space(&mut scene, 2, Affine2::rotation(0.25).to_matrix4());

    scene.push_shadow(Shadow::drop_shadow(
        card,
        (0.0, 2.0),
        1.0,
        4.0,
        Color::srgb(0.0, 0.0, 0.0, 0.25),
    ));
    scene.push_quad(
        Quad::filled(card, ramp)
            .with_radii(Corners::uniform(Vec2::splat(DevicePx(6.0))))
            .with_border([2.0; 4], flat, BorderStyle::Dashed),
    );
    scene.push_decoration(
        Decoration::new(
            rect(16.0, 40.0, 64.0, 2.0),
            1.0,
            Color::srgb(0.9, 0.1, 0.1, 1.0),
            DecorationStyle::Wavy,
        )
        .clipped(rounded),
    );
    scene.push_mono_sprite(
        MonoSprite::new(
            rect(16.0, 16.0, 8.0, 16.0),
            tile(TextureKind::Mono, 0),
            Color::BLACK,
        )
        .clipped(rounded),
    );
    scene.push_subpixel_sprite(SubpixelSprite::new(
        rect(32.0, 16.0, 8.0, 16.0),
        tile(TextureKind::Subpixel, 1),
        Color::BLACK,
    ));
    scene.push_color_sprite(
        ColorSprite::new(rect(48.0, 16.0, 16.0, 16.0), tile(TextureKind::Color, 2))
            .with_radii(Corners::uniform(Vec2::splat(DevicePx(8.0))))
            .clipped(masked),
    );
    scene.push_external(ExternalQuad::new(
        rect(160.0, 16.0, 64.0, 48.0),
        ExternalTextureId(7),
    ));

    let group = GroupBoundary::start(
        rect(8.0, 24.0, 120.0, 80.0),
        0.5,
        BlendMode::new(Mix::Multiply, Compose::SrcOver),
        smallvec![Filter::Blur(3.0)],
    );
    scene.push_group(group.clone());
    // Inside the card's clip: content its clip admits nothing of never reaches the display list at
    // all, which would leave this scene with no vector work and the pass count trivially zero.
    let path = Arc::new(kurbo::Circle::new((64.0, 56.0), 24.0).to_path(0.1));
    scene.push_vector(
        VectorItem::filled(VectorId(1), path, flat)
            .clipped(rounded)
            .even_odd(),
    );
    scene.push_group(group.end());

    scene.push_backdrop(BackdropFilter::new(
        rect(140.0, 120.0, 100.0, 60.0),
        smallvec![Filter::Saturate(1.8), Filter::Blur(2.0)],
    ));

    let mut transformed = Quad::filled(rect(200.0, 200.0, 32.0, 32.0), flat);
    transformed.transform = spun.index();
    scene.push_quad(transformed);

    scene.finish(&DamageSet::full());
    scene
}

/// A coordinate system directly under the viewport, holding `matrix`.
///
/// Named after a made-up owner, because a scene built by hand has no boxes to name one after.
fn space(scene: &mut Scene, owner: u64, matrix: zgui_geom::Matrix4) -> zgui_scene::SpatialId {
    let viewport = scene.spatial.viewport();
    let owner = zgui_scene::PropertyOwner::new(owner).expect("not the empty word");
    let own = zgui_scene::OwnSpace::of(Some(matrix), None, false);
    scene.spatial.space_of(viewport, owner, own)
}
