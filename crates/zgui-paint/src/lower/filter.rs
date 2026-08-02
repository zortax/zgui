//! Everything that decides whether a subtree composites as a unit: filters, opacity, blending and
//! isolation.
//!
//! The filter chains themselves are converted by the layout stage, because how far a filter reaches
//! decides a fragment's ink and its membership of the read-extent registry — two answers that are
//! needed before anything is drawn. A second conversion here would let the ink, the registry and the
//! damage disagree, so this calls that one.

use zgui_css::parity::Support;
use zgui_css::values::effect::{IsolationValue, MixBlendModeValue};
use zgui_css::{ComputedStyle, register_properties};
use zgui_layout::fragment::filter::FilterChain;
use zgui_scene::peniko;

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
    pub blend: peniko::BlendMode,
    /// Whether the box refuses to blend with anything outside itself.
    pub isolated: bool,
}

impl GroupPaint {
    /// Whether anything here forces a target of its own, ignoring the geometric opacity fold.
    ///
    /// The fold is decided elsewhere and needs the subtree, so it is deliberately not part of this:
    /// a blend mode, a filter or an explicit isolation needs a boundary whatever the geometry says,
    /// and opacity is the one that sometimes does not.
    pub fn needs_isolation(&self) -> bool {
        !self.filters.is_empty()
            || !self.backdrop.is_empty()
            || self.isolated
            || self.blend != peniko::BlendMode::default()
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
    }
}

/// The compositing mode a `mix-blend-mode` keyword means.
///
/// All sixteen of CSS's separable and non-separable modes have an exact counterpart, and
/// `plus-lighter` is the one that is a Porter-Duff compose rather than a blend.
pub fn blend_of(mode: MixBlendModeValue) -> peniko::BlendMode {
    use peniko::{BlendMode, Compose, Mix};
    match mode {
        MixBlendModeValue::Normal => BlendMode::default(),
        MixBlendModeValue::Multiply => BlendMode::new(Mix::Multiply, Compose::SrcOver),
        MixBlendModeValue::Screen => BlendMode::new(Mix::Screen, Compose::SrcOver),
        MixBlendModeValue::Overlay => BlendMode::new(Mix::Overlay, Compose::SrcOver),
        MixBlendModeValue::Darken => BlendMode::new(Mix::Darken, Compose::SrcOver),
        MixBlendModeValue::Lighten => BlendMode::new(Mix::Lighten, Compose::SrcOver),
        MixBlendModeValue::ColorDodge => BlendMode::new(Mix::ColorDodge, Compose::SrcOver),
        MixBlendModeValue::ColorBurn => BlendMode::new(Mix::ColorBurn, Compose::SrcOver),
        MixBlendModeValue::HardLight => BlendMode::new(Mix::HardLight, Compose::SrcOver),
        MixBlendModeValue::SoftLight => BlendMode::new(Mix::SoftLight, Compose::SrcOver),
        MixBlendModeValue::Difference => BlendMode::new(Mix::Difference, Compose::SrcOver),
        MixBlendModeValue::Exclusion => BlendMode::new(Mix::Exclusion, Compose::SrcOver),
        MixBlendModeValue::Hue => BlendMode::new(Mix::Hue, Compose::SrcOver),
        MixBlendModeValue::Saturation => BlendMode::new(Mix::Saturation, Compose::SrcOver),
        MixBlendModeValue::Color => BlendMode::new(Mix::Color, Compose::SrcOver),
        MixBlendModeValue::Luminosity => BlendMode::new(Mix::Luminosity, Compose::SrcOver),
        MixBlendModeValue::PlusLighter => BlendMode::new(Mix::Normal, Compose::Plus),
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::values::effect::MixBlendModeValue;
    use zgui_scene::peniko;

    use super::blend_of;

    #[test]
    fn normal_is_the_mode_that_needs_no_boundary() {
        assert_eq!(
            blend_of(MixBlendModeValue::Normal),
            peniko::BlendMode::default()
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
        let mut seen: Vec<peniko::BlendMode> = vec![blend_of(MixBlendModeValue::Normal)];
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
