//! Everything that decides whether a subtree composites as a unit: filters, opacity, blending and
//! isolation.
//!
//! The filter chains themselves are converted by the layout stage, because how far a filter reaches
//! decides a fragment's ink and its membership of the read-extent registry — two answers that are
//! needed before anything is drawn. A second conversion here would let the ink, the registry and the
//! damage disagree, so this calls that one.

use compositor_types::BlendMode;
use zgui_css::parity::Support;
use zgui_css::values::effect::{IsolationValue, MixBlendModeValue};
use zgui_css::{ComputedStyle, register_properties};
use zgui_layout::fragment::filter::FilterChain;

register_properties! {
    opacity         => Support::Implemented("zgui-paint::lower::filter"),
    filter          => Support::Implemented("zgui-paint::lower::filter"),
    backdrop_filter => Support::Implemented("zgui-paint::lower::filter"),
    mix_blend_mode  => Support::Implemented("zgui-paint::lower::filter"),
    isolation       => Support::Implemented("zgui-paint::lower::filter"),
}

/// What makes a box composite on its own, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPaint {
    /// A multiplier on the whole subtree's alpha.
    pub opacity: f32,
    /// The filters applied to the box's own content.
    pub filters: FilterChain,
    /// The filters applied to whatever is drawn beneath the box.
    pub backdrop: FilterChain,
    /// How the box composites onto what is beneath it.
    pub blend: BlendMode,
    /// Whether the box refuses to blend with anything outside itself.
    pub isolated: bool,
    /// Whether the box declares a `filter` or a `backdrop-filter`.
    ///
    /// Such a box isolates the blending of its descendants, also when each filter in the chain is
    /// an identity such as `blur(0)` and the chains above are empty.
    pub isolates_blending: bool,
}

impl GroupPaint {
    /// Whether anything here forces a target of its own, ignoring the geometric opacity fold.
    ///
    /// The fold is decided elsewhere and needs the subtree, so it is deliberately not part of this:
    /// a blend mode, a filter or an explicit isolation needs a boundary whatever the geometry says,
    /// and opacity is the one that sometimes does not. A backdrop filter is also absent: it reads
    /// what is beneath the box and composites over it, and needs a boundary only for a descendant
    /// that reads what is behind it too (see [`isolation`](crate::emit::group::isolation)).
    pub fn needs_isolation(&self) -> bool {
        !self.filters.is_empty() || self.isolated || self.blend != BlendMode::NORMAL
    }
}

/// Lowers the group properties of a style.
pub fn of(style: &ComputedStyle, scale: f32) -> GroupPaint {
    let effects = style.get_effects();
    GroupPaint {
        opacity: effects.opacity.clamp(0.0, 1.0),
        filters: zgui_layout::fragment::filter::own(style, scale),
        backdrop: zgui_layout::fragment::filter::backdrop(style, scale),
        blend: blend_of(effects.mix_blend_mode),
        isolated: style.get_box().isolation == IsolationValue::Isolate,
        isolates_blending: !effects.filter.0.is_empty() || !effects.backdrop_filter.0.is_empty(),
    }
}

/// The compositing mode a `mix-blend-mode` keyword means.
///
/// All sixteen of CSS's separable and non-separable modes have an exact counterpart, and
/// `plus-lighter` is the one that is a Porter-Duff compose rather than a blend.
pub fn blend_of(mode: MixBlendModeValue) -> BlendMode {
    match mode {
        MixBlendModeValue::Normal => BlendMode::NORMAL,
        MixBlendModeValue::Multiply => BlendMode::MULTIPLY,
        MixBlendModeValue::Screen => BlendMode::SCREEN,
        MixBlendModeValue::Overlay => BlendMode::OVERLAY,
        MixBlendModeValue::Darken => BlendMode::DARKEN,
        MixBlendModeValue::Lighten => BlendMode::LIGHTEN,
        MixBlendModeValue::ColorDodge => BlendMode::COLOR_DODGE,
        MixBlendModeValue::ColorBurn => BlendMode::COLOR_BURN,
        MixBlendModeValue::HardLight => BlendMode::HARD_LIGHT,
        MixBlendModeValue::SoftLight => BlendMode::SOFT_LIGHT,
        MixBlendModeValue::Difference => BlendMode::DIFFERENCE,
        MixBlendModeValue::Exclusion => BlendMode::EXCLUSION,
        MixBlendModeValue::Hue => BlendMode::HUE,
        MixBlendModeValue::Saturation => BlendMode::SATURATION,
        MixBlendModeValue::Color => BlendMode::COLOR,
        MixBlendModeValue::Luminosity => BlendMode::LUMINOSITY,
        MixBlendModeValue::PlusLighter => BlendMode::PLUS_LIGHTER,
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::values::effect::MixBlendModeValue;

    use super::blend_of;

    #[test]
    fn normal_is_the_mode_that_needs_no_boundary() {
        assert_eq!(
            blend_of(MixBlendModeValue::Normal),
            compositor_types::BlendMode::NORMAL
        );
    }

    #[test]
    fn every_other_mode_differs_from_normal_and_from_the_one_before_it() {
        let modes = [
            MixBlendModeValue::Multiply,
            MixBlendModeValue::Screen,
            MixBlendModeValue::Overlay,
            MixBlendModeValue::Darken,
            MixBlendModeValue::Lighten,
            MixBlendModeValue::ColorDodge,
            MixBlendModeValue::ColorBurn,
            MixBlendModeValue::HardLight,
            MixBlendModeValue::SoftLight,
            MixBlendModeValue::Difference,
            MixBlendModeValue::Exclusion,
            MixBlendModeValue::Hue,
            MixBlendModeValue::Saturation,
            MixBlendModeValue::Color,
            MixBlendModeValue::Luminosity,
            MixBlendModeValue::PlusLighter,
        ];
        let mut seen: Vec<compositor_types::BlendMode> = vec![blend_of(MixBlendModeValue::Normal)];
        for mode in modes {
            let blend = blend_of(mode);
            assert!(
                !seen.contains(&blend),
                "{mode:?} composites the same way as something already mapped"
            );
            seen.push(blend);
        }
        assert_eq!(seen.len(), 17, "sixteen modes plus normal");
    }
}
