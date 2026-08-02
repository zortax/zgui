//! Content this engine does not lay out: images, video frames, embedded surfaces.
//!
//! A replaced box draws two things and they come from different places. Its box decorations are
//! ordinary — a background, a border, a shadow — and are emitted like any other box's. Its
//! *content* is a texture somebody else owns, and where that texture lives decides which primitive
//! carries it: a decoded image is a tile in this framework's own atlas, and a video frame or a
//! surface from another process is not, so it is drawn from the texture it is already in.

use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};
use zgui_scene::{
    ClipId, ColorSprite, ExternalQuad, ExternalTextureId, Resource, Scene, SpatialId,
};

/// Where a replaced box's content lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A decoded image, in this framework's own atlas.
    ///
    /// Its texels are premultiplied, because the decode premultiplies once and the cache holds that
    /// form — so a re-upload after a lost device cannot reintroduce straight alpha.
    Decoded(Resource),
    /// A texture owned by something else, drawn from where it already is.
    External(ExternalTextureId),
}

/// Where a replaced box's content is drawn.
#[derive(Clone, Copy, Debug)]
pub struct ReplacedPlacement {
    /// The content box, in absolute device pixels.
    pub content_box: Rect<DevicePx, Device>,
    /// The corner radii the content is cut to, which are the padding box's.
    pub radii: Corners<Vec2<DevicePx>>,
    /// The chain the content is drawn through.
    pub clip: ClipId,
    /// The transform it is drawn under.
    pub transform: SpatialId,
    /// A multiplier on the content's own alpha.
    pub opacity: f32,
}

/// Emits a replaced box's content, and returns how many primitives were pushed.
///
/// Nothing is drawn for a content box with no area, which is what an image sized to zero by its
/// container is — and pushing one would cost a spatial-index query to draw nothing.
pub fn emit(scene: &mut Scene, source: Source, placement: ReplacedPlacement) -> usize {
    if placement.content_box.is_empty() {
        return 0;
    }
    let landed = match source {
        Source::Decoded(resource) => {
            let mut sprite = ColorSprite::new(placement.content_box, resource)
                .clipped(placement.clip)
                .with_radii(placement.radii);
            sprite.transform = placement.transform.index();
            sprite.opacity = placement.opacity;
            scene.push_color_sprite(sprite).is_some()
        }
        Source::External(texture) => {
            let mut quad =
                ExternalQuad::new(placement.content_box, texture).clipped(placement.clip);
            quad.transform = placement.transform;
            quad.opacity = placement.opacity;
            scene.push_external(quad).is_some()
        }
    };
    usize::from(landed)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
    use zgui_scene::{ClipId, ExternalTextureId, Scene, SpatialId};

    use super::{ReplacedPlacement, Source, emit};

    /// A placement over the given content box.
    fn placement(rect: Rect<DevicePx, Device>) -> ReplacedPlacement {
        ReplacedPlacement {
            content_box: rect,
            radii: Corners::uniform(Vec2::splat(DevicePx(0.0))),
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            opacity: 1.0,
        }
    }

    #[test]
    fn an_external_texture_reaches_the_display_list() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(64, 64));
        let rect = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(32.0), DevicePx(32.0)),
        );
        assert_eq!(
            emit(
                &mut scene,
                Source::External(ExternalTextureId(7)),
                placement(rect)
            ),
            1
        );
        assert_eq!(scene.primitives.externals.len(), 1);
    }

    #[test]
    fn a_content_box_with_no_area_draws_nothing() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(64, 64));
        let empty = Rect::new(
            Point::new(DevicePx(4.0), DevicePx(4.0)),
            Size::new(DevicePx(0.0), DevicePx(16.0)),
        );
        assert_eq!(
            emit(
                &mut scene,
                Source::External(ExternalTextureId(1)),
                placement(empty)
            ),
            0
        );
        assert!(scene.primitives.externals.is_empty());
    }
}
