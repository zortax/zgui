//! Content this engine does not lay out: images, video frames, embedded surfaces.
//!
//! A replaced box draws two things and they come from different places. Its box decorations are
//! ordinary, a background, a border or a shadow, and are emitted like any other box's. Its content
//! is either decoded texels, which the compositor keeps as an image, or a video frame or a surface
//! from another process, which is drawn from the texture it is already in.

use std::sync::Arc;

use iceblink::{
    Extend, ExternalQuad, Image, ImageDesc, ImageFilter, ImageFlags, ImageFormat, PaintRef,
    TextureId,
};
use zgui_css::values::size::{ObjectFitValue, ObjectPositionValue};
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_space::{ClipId, SpatialId};

use crate::emit::record::{ImageKey, ImagePixels, Recorder, flat_radii, local_rect};

/// Where a replaced box's content lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Decoded texels, premultiplied, gamma-encoded sRGB, four bytes per texel.
    Decoded {
        /// The image the compositor keeps the texels as.
        key: ImageKey,
        /// The extent of the texels.
        size: Size<u32, Device>,
        /// The image's own extent, read as CSS pixels, which may be larger than the texels, whose
        /// extent is sized by the box the image is shown in. `object-fit: none` draws at this
        /// size, and the fitted rectangle's aspect comes from it.
        natural: Size<u32, Device>,
        /// The texels, or `None` once their host copy has been given back.
        texels: Option<Arc<Vec<u8>>>,
        /// A number that moves whenever the texels under `key` change.
        revision: u64,
    },
    /// A texture owned by something else, drawn from where it already is, as an external
    /// texture slot of the window's compositor client.
    External(TextureId),
}

/// Where a replaced box's content is drawn.
#[derive(Clone, Debug)]
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
    /// How the content meets the content box.
    pub fit: ObjectFitValue,
    /// Where the fitted content sits in the box's leftover space.
    pub position: ObjectPositionValue,
    /// Device pixels per CSS pixel, which scales the content's natural extent and any `px`
    /// offset in the position.
    pub scale: f32,
}

/// Records a replaced box's content, and returns how many items were recorded and primitives
/// pushed.
///
/// Nothing is drawn for a content box with no area, which is what an image sized to zero by its
/// container is.
///
/// Returns `None` when the texels' host copy has been given back and the compositor holds no image
/// of them.
pub fn emit(rec: &mut Recorder<'_>, source: Source, placement: ReplacedPlacement) -> Option<usize> {
    if placement.content_box.is_empty() {
        return Some(0);
    }
    match source {
        Source::Decoded {
            key,
            size,
            natural,
            texels,
            revision,
        } => {
            let pixels = texels.as_deref().map(|texels| ImagePixels {
                desc: ImageDesc {
                    width: size.width,
                    height: size.height,
                    format: ImageFormat::RGBA8_PREMULTIPLIED,
                },
                revision,
                texels: Some(texels.as_slice()),
            });
            let (image, [width, height]) = rec.image_slot(key, pixels)?;
            let item = Image {
                bounds: local_rect(destination(&placement, natural)),
                frame: local_rect(placement.content_box),
                source: [0.0, 0.0, width as f32, height as f32],
                radii: flat_radii(placement.radii),
                shapes: iceblink::ROUND,
                paint: PaintRef::NONE,
                image,
                opacity: placement.opacity,
                clip: rec.clip(placement.clip),
                space: rec.space(placement.transform),
                flags: ImageFlags::default(),
                extend: [Extend::NONE; 2],
                filter: ImageFilter::LINEAR,
            };
            rec.image(item);
            Some(1)
        }
        Source::External(texture) => {
            let quad = ExternalQuad {
                bounds: local_rect(placement.content_box),
                texture: 0,
                opacity: placement.opacity,
                clip: rec.clip(placement.clip),
                space: rec.space(placement.transform),
            };
            rec.external(texture, quad);
            Some(1)
        }
    }
}

/// The rectangle `object-fit` and `object-position` place the content at, in device pixels.
///
/// The image is drawn over this rectangle and cut to the content box: a `cover` overflow is cut
/// by the frame, and a `contain` letterbox simply draws nothing where the content is not.
fn destination(
    placement: &ReplacedPlacement,
    natural: Size<u32, Device>,
) -> Rect<DevicePx, Device> {
    let box_ = placement.content_box;
    let (box_w, box_h) = (box_.size.width.0, box_.size.height.0);
    let natural_w = natural.width as f32 * placement.scale;
    let natural_h = natural.height as f32 * placement.scale;
    if natural_w <= 0.0 || natural_h <= 0.0 {
        return box_;
    }
    let (width, height) = match placement.fit {
        ObjectFitValue::Fill => (box_w, box_h),
        ObjectFitValue::Contain => {
            let scale = (box_w / natural_w).min(box_h / natural_h);
            (natural_w * scale, natural_h * scale)
        }
        ObjectFitValue::Cover => {
            let scale = (box_w / natural_w).max(box_h / natural_h);
            (natural_w * scale, natural_h * scale)
        }
        ObjectFitValue::None => (natural_w, natural_h),
        ObjectFitValue::ScaleDown => {
            let scale = (box_w / natural_w).min(box_h / natural_h).min(1.0);
            (natural_w * scale, natural_h * scale)
        }
    };
    let x = box_.origin.x.0
        + offset(
            &placement.position.horizontal,
            box_w - width,
            placement.scale,
        );
    let y = box_.origin.y.0
        + offset(
            &placement.position.vertical,
            box_h - height,
            placement.scale,
        );
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// One axis of `object-position`, resolved against the box's leftover space.
///
/// The value computes in CSS pixels — a percentage of the leftover, or a length — so the leftover
/// is taken to CSS pixels for the resolution and the answer scaled back, exactly as the layout
/// engine resolves its own calc expressions.
fn offset(position: &zgui_css::values::length::LengthPercentage, leftover: f32, scale: f32) -> f32 {
    position
        .resolve(zgui_css::values::length::Length::new(leftover / scale))
        .px()
        * scale
}

#[cfg(test)]
mod tests {
    use crate::Tables;
    use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
    use zgui_space::{ClipId, SpatialId};

    use super::{ReplacedPlacement, Source, emit};
    use crate::emit::record::Recording;

    /// A placement over the given content box, with the initial fit: `fill` at `50% 50%`.
    fn placement(rect: Rect<DevicePx, Device>) -> ReplacedPlacement {
        ReplacedPlacement {
            content_box: rect,
            radii: Corners::uniform(Vec2::splat(DevicePx(0.0))),
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            opacity: 1.0,
            fit: zgui_css::values::size::ObjectFitValue::Fill,
            position: zgui_css::values::size::ObjectPositionValue::center(),
            scale: 1.0,
        }
    }

    /// An external texture slot of a client.
    fn texture() -> iceblink::TextureId {
        iceblink::IdSpace::new(iceblink::ClientId(1)).mint()
    }

    #[test]
    fn an_external_texture_is_recorded_over_the_content_box() {
        let mut tables = Tables::new();
        tables.begin_frame(Size::new(64, 64));
        let rect = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(32.0), DevicePx(32.0)),
        );
        let mut recording = Recording::new();
        let mut rec = recording.recorder(&mut tables, SpatialId::VIEWPORT, ClipId::ROOT);
        assert_eq!(
            emit(&mut rec, Source::External(texture()), placement(rect)),
            Some(1)
        );
        let [external] = recording.externals() else {
            panic!("one external quad: {:?}", recording.externals());
        };
        assert_eq!(external.bounds, [0.0, 0.0, 32.0, 32.0]);
    }

    /// Every fit mode, against one box and one picture whose aspects disagree.
    ///
    /// A 200×100 box and a 100×200 picture: the interesting case, because every mode answers it
    /// differently. The position stays at its initial `50% 50%`, so leftover space splits evenly.
    #[test]
    fn each_fit_mode_places_the_picture_where_css_says() {
        use zgui_css::values::size::ObjectFitValue;

        let rect = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(200.0), DevicePx(100.0)),
        );
        let natural = Size::new(100u32, 200u32);
        let at = |fit: ObjectFitValue| {
            let mut placement = placement(rect);
            placement.fit = fit;
            let destination = super::destination(&placement, natural);
            [
                destination.origin.x.0,
                destination.origin.y.0,
                destination.size.width.0,
                destination.size.height.0,
            ]
        };

        assert_eq!(at(ObjectFitValue::Fill), [0.0, 0.0, 200.0, 100.0]);
        assert_eq!(
            at(ObjectFitValue::Contain),
            [75.0, 0.0, 50.0, 100.0],
            "contain fits the tall picture to the box height and centres the leftover"
        );
        assert_eq!(
            at(ObjectFitValue::Cover),
            [0.0, -150.0, 200.0, 400.0],
            "cover fills the box width and lets the height overflow into the frame's cut"
        );
        assert_eq!(
            at(ObjectFitValue::None),
            [50.0, -50.0, 100.0, 200.0],
            "none draws at the natural size, centred"
        );
        assert_eq!(
            at(ObjectFitValue::ScaleDown),
            [75.0, 0.0, 50.0, 100.0],
            "scale-down is contain for a picture larger than its box"
        );

        let small = Size::new(20u32, 10u32);
        let mut placement = placement(rect);
        placement.fit = ObjectFitValue::ScaleDown;
        let destination = super::destination(&placement, small);
        assert_eq!(
            [
                destination.origin.x.0,
                destination.origin.y.0,
                destination.size.width.0,
                destination.size.height.0,
            ],
            [90.0, 45.0, 20.0, 10.0],
            "and none for a picture smaller than its box"
        );
    }

    /// A doubled device scale doubles the natural extent and any length offset.
    #[test]
    fn the_device_scale_reaches_the_natural_size() {
        let rect = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(200.0), DevicePx(100.0)),
        );
        let mut placement = placement(rect);
        placement.fit = zgui_css::values::size::ObjectFitValue::None;
        placement.scale = 2.0;
        let destination = super::destination(&placement, Size::new(50u32, 25u32));
        assert_eq!(
            [destination.size.width.0, destination.size.height.0],
            [100.0, 50.0],
            "fifty CSS pixels are a hundred device pixels at 2×"
        );
    }

    #[test]
    fn a_content_box_with_no_area_draws_nothing() {
        let mut tables = Tables::new();
        tables.begin_frame(Size::new(64, 64));
        let empty = Rect::new(
            Point::new(DevicePx(4.0), DevicePx(4.0)),
            Size::new(DevicePx(0.0), DevicePx(16.0)),
        );
        let mut recording = Recording::new();
        let mut rec = recording.recorder(&mut tables, SpatialId::VIEWPORT, ClipId::ROOT);
        assert_eq!(
            emit(&mut rec, Source::External(texture()), placement(empty)),
            Some(0)
        );
        assert!(recording.externals().is_empty());
    }
}
