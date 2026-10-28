//! Box shadows, drop and inset.

use bytemuck::{Pod, Zeroable};
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Rect};

use crate::id::{ClipId, DrawOrder};
use crate::prim::layout::rect_of;
use crate::spatial::SpatialId;

/// A blurred rounded rectangle, cast by a box.
///
/// One struct serves both `box-shadow` forms. The blurred shape is `shape_bounds` with `radii`. A
/// drop shadow paints that shape outside the box that cast it, so its `bounds` is the shape dilated
/// by the blur. An inset shadow paints the complement of that shape inside the box, so its `bounds`
/// is the box itself. Either way `bounds` is what the primitive paints, which is what draw order
/// and culling are computed from.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Shadow {
    /// Where this draws in the painting order.
    pub order: DrawOrder,
    /// The blur's standard deviation, in device pixels.
    pub blur: f32,
    /// Everything this paints, as `[x, y, width, height]`.
    pub bounds: [f32; 4],
    /// The shape the blur is applied to, as `[x, y, width, height]`.
    ///
    /// It is the casting box offset and grown by the spread for a drop shadow, and the hole the
    /// shadow leaves open for an inset one: the box offset and shrunk by the spread.
    pub shape_bounds: [f32; 4],
    /// The shadow shape's elliptical corner radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The casting box, as `[x, y, width, height]`.
    pub element_bounds: [f32; 4],
    /// The casting box's elliptical corner radii.
    pub element_radii: [f32; 8],
    /// Premultiplied, gamma-encoded sRGB.
    pub color: [f32; 4],
    /// The [`ClipId`] this draws through.
    pub clip: u32,
    /// The slot of the [`SpatialId`] this draws under.
    pub transform: u32,
    /// One when the shadow is inset, zero when it is cast outwards.
    pub inset: u32,
    /// The superellipse exponent the element's corners are cut with; two is the ellipse.
    ///
    /// A shadow is the element's own shape blurred, so it has to be cut the same way: a squircle
    /// casting a rounded-rectangle shadow shows the shadow's corners outside its own.
    ///
    /// This was the padding word the structure needed to be copied as bytes, which is why adding
    /// it costs a shadow nothing.
    pub shape: f32,
}

impl Shadow {
    /// How many standard deviations of blur a shadow visibly reaches, at any alpha.
    ///
    /// Three is where a Gaussian falls below one part in a thousand, which is under half a level at
    /// eight bits per channel: dilating by less leaves a visible edge where the shadow is cut off,
    /// and dilating by more costs pixels that cannot be seen.
    ///
    /// That is the bound for a shadow drawn **opaque**. One drawn at a lower alpha reaches less far,
    /// because what has to clear half a level is the shadow's contribution and not the Gaussian
    /// alone — [`Shadow::blur_extent`] is that, and this is what it never exceeds.
    pub const BLUR_EXTENT: f32 = 3.0;

    /// How many standard deviations of blur a shadow drawn at `alpha` visibly reaches.
    ///
    /// A pixel is worth covering while the shadow still moves it by half a level of the eight the
    /// channel has, so what must fall below `0.5 / 255` is `alpha` times the Gaussian's tail rather
    /// than the tail alone. Solving
    ///
    /// ```text
    /// exp(-k^2 / 2) / (k * sqrt(2 * pi)) = 0.5 / (255 * alpha)
    /// ```
    ///
    /// for `k` answers a `k` whose *true* tail is below that, because the left side is an upper
    /// bound on the tail rather than the tail itself. So this errs outwards, which is the direction
    /// that cannot clip a shadow.
    ///
    /// An opaque shadow answers 2.91, one at 0.55 answers 2.73 — about a tenth off every side — and
    /// one at 0.25 answers 2.46. Below `0.5 / 255` the shadow cannot move any pixel by half a level
    /// anywhere, and the answer is zero.
    ///
    /// **Never more than [`Shadow::BLUR_EXTENT`]**, so that taking the alpha into account can only
    /// ever shrink what a shadow claims and never grow it.
    pub fn blur_extent(alpha: f32) -> f32 {
        /// What the contribution has to fall below: half a level of an eight-bit channel.
        const HALF_LEVEL: f32 = 0.5 / 255.0;
        /// Enough for the fixed point below to settle; it moves by under a thousandth by the third.
        const STEPS: usize = 4;

        // Compared through `partial_cmp` rather than negated, so that an alpha which is not a
        // number takes this branch instead of reaching the fixed point below.
        if alpha.partial_cmp(&HALF_LEVEL) != Some(core::cmp::Ordering::Greater) {
            return 0.0;
        }
        let tail = HALF_LEVEL / alpha;
        let root_two_pi = core::f32::consts::TAU.sqrt();
        // Rearranged as `k = sqrt(-2 * ln(tail * k * sqrt(2 * pi)))` and iterated from the opaque
        // answer, which is the largest `k` can be and so approaches the root from outside.
        let mut k = Self::BLUR_EXTENT;
        for _ in 0..STEPS {
            let inner = tail * k * root_two_pi;
            if inner >= 1.0 {
                return 0.0;
            }
            k = (-2.0 * inner.ln()).max(0.0).sqrt();
        }
        k.clamp(0.0, Self::BLUR_EXTENT)
    }

    /// A shadow cast outwards from `element`, blurred by `blur` standard deviations.
    ///
    /// The painted extent is derived here rather than taken from the caller, so that the rectangle
    /// culling and ordering use is the rectangle the shader actually covers.
    pub fn drop_shadow(
        element: Rect<DevicePx, Device>,
        offset: (f32, f32),
        spread: f32,
        blur: f32,
        color: Color,
    ) -> Self {
        let shape = [
            element.origin.x.0 + offset.0 - spread,
            element.origin.y.0 + offset.1 - spread,
            element.size.width.0 + 2.0 * spread,
            element.size.height.0 + 2.0 * spread,
        ];
        let reach = Self::blur_extent(color.alpha()) * blur;
        Self {
            order: 0,
            blur,
            bounds: [
                shape[0] - reach,
                shape[1] - reach,
                shape[2] + 2.0 * reach,
                shape[3] + 2.0 * reach,
            ],
            shape_bounds: shape,
            radii: [0.0; 8],
            element_bounds: [
                element.origin.x.0,
                element.origin.y.0,
                element.size.width.0,
                element.size.height.0,
            ],
            element_radii: [0.0; 8],
            color: color.to_premultiplied_srgb(),
            clip: ClipId::ROOT.0,
            transform: SpatialId::VIEWPORT.index(),
            inset: 0,
            shape: crate::prim::CornerShape::ROUND.get(),
        }
    }

    /// A shadow cast inwards, which paints only inside `element`.
    ///
    /// `element` is the box the shadow is clipped to, which is the padding box in CSS. The hole is
    /// that box offset and shrunk by `spread`. A spread past the box's half extent closes the hole,
    /// and the shadow then covers the whole box.
    pub fn inset_shadow(
        element: Rect<DevicePx, Device>,
        offset: (f32, f32),
        spread: f32,
        blur: f32,
        color: Color,
    ) -> Self {
        let mut shadow = Self::drop_shadow(element, offset, 0.0, blur, color);
        let width = (element.size.width.0 - 2.0 * spread).max(0.0);
        let height = (element.size.height.0 - 2.0 * spread).max(0.0);
        let center_x = element.origin.x.0 + offset.0 + element.size.width.0 * 0.5;
        let center_y = element.origin.y.0 + offset.1 + element.size.height.0 * 0.5;
        shadow.shape_bounds = [
            center_x - width * 0.5,
            center_y - height * 0.5,
            width,
            height,
        ];
        shadow.inset = 1;
        shadow.bounds = shadow.element_bounds;
        shadow
    }

    /// The same shadow cast by an element whose corners are cut to `shape`.
    pub fn with_corner_shape(mut self, shape: crate::prim::CornerShape) -> Self {
        self.shape = shape.get();
        self
    }

    /// The same shadow drawn through `clip`.
    pub fn clipped(mut self, clip: ClipId) -> Self {
        self.clip = clip.0;
        self
    }

    /// The rectangle this paints.
    pub fn ink(&self) -> Rect<DevicePx, Device> {
        rect_of(self.bounds)
    }

    /// The clip chain this draws through.
    pub fn clip_id(&self) -> ClipId {
        ClipId(self.clip)
    }
}

#[cfg(test)]
mod tests {
    //! What a shadow's reach comes to, and the two properties the rest of the frame depends on.

    use zgui_color::Color;

    use super::Shadow;

    /// The reach at `alpha`, to three places.
    fn reach(alpha: f32) -> f32 {
        (Shadow::blur_extent(alpha) * 1000.0).round() / 1000.0
    }

    #[test]
    fn a_fainter_shadow_reaches_less_far() {
        // The whole point: an opaque shadow still moves a pixel by half a level nearly three
        // deviations out, and one at 0.55 stops sooner, which is a tenth off every side.
        assert_eq!(reach(1.0), 2.914);
        assert_eq!(reach(0.55), 2.726);
        assert_eq!(reach(0.25), 2.461);
        assert!(
            reach(0.55) < reach(1.0),
            "a fainter shadow claimed at least as much as an opaque one"
        );
    }

    #[test]
    fn no_alpha_ever_reaches_further_than_an_opaque_shadow() {
        // The safety property. Taking the alpha into account may only shrink what a shadow claims:
        // anything else would be a rectangle the old one did not cover, and the shader is sized by
        // the same number.
        for step in 0..=1000 {
            let alpha = step as f32 / 1000.0;
            let reach = Shadow::blur_extent(alpha);
            assert!(
                (0.0..=Shadow::BLUR_EXTENT).contains(&reach),
                "alpha {alpha} reached {reach}, outside nothing-to-{}",
                Shadow::BLUR_EXTENT
            );
        }
    }

    #[test]
    fn a_shadow_too_faint_to_move_any_pixel_reaches_nothing() {
        // Below half a level there is no distance at which it shows, so there is nothing to dilate
        // by. Guarded because the solve has no root there and would otherwise answer a NaN.
        assert_eq!(Shadow::blur_extent(0.0), 0.0);
        assert_eq!(Shadow::blur_extent(0.5 / 255.0), 0.0);
        assert_eq!(Shadow::blur_extent(-1.0), 0.0);
        assert_eq!(Shadow::blur_extent(f32::NAN), 0.0);
    }

    #[test]
    fn the_bounds_a_shadow_reports_follow_the_alpha_it_is_drawn_with() {
        // The reach reaches `drop_shadow`, which is what damage and culling are taken from.
        let element = zgui_geom::Rect::new(
            zgui_geom::Point::new(zgui_geom::DevicePx(100.0), zgui_geom::DevicePx(100.0)),
            zgui_geom::Size::new(zgui_geom::DevicePx(50.0), zgui_geom::DevicePx(50.0)),
        );
        let opaque = Shadow::drop_shadow(
            element,
            (0.0, 0.0),
            0.0,
            10.0,
            Color::srgb(0.0, 0.0, 0.0, 1.0),
        );
        let faint = Shadow::drop_shadow(
            element,
            (0.0, 0.0),
            0.0,
            10.0,
            Color::srgb(0.0, 0.0, 0.0, 0.55),
        );
        assert!(
            faint.bounds[2] < opaque.bounds[2],
            "the fainter shadow claimed {} against the opaque one's {}",
            faint.bounds[2],
            opaque.bounds[2]
        );
    }
}
