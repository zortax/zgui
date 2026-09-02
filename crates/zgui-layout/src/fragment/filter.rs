//! A computed filter chain, in the vocabulary the display list speaks.
//!
//! One conversion, one home. How far a filter reaches decides three different things — how far a
//! fragment's ink spreads, whether it belongs in the registry of fragments that read pixels they do
//! not write, and how far the damage around it has to grow — and two of those are answered before
//! anything is drawn. A second conversion elsewhere would let those three disagree.

use smallvec::SmallVec;
use zgui_css::ComputedStyle;
use zgui_css::values::effect::FilterValue;
use zgui_scene::Filter;

/// A filter chain, which is short in every document that has one at all.
pub type FilterChain = SmallVec<[Filter; 2]>;

/// The `filter` chain applied to a box's own content.
pub fn own(style: &ComputedStyle, scale: f32) -> FilterChain {
    let mut chain = chain(&style.get_effects().filter.0, scale);
    chain.extend(effect(style, zgui_scene::property::FILTER, scale));
    chain
}

/// The `backdrop-filter` chain a box samples what is behind it through.
pub fn backdrop(style: &ComputedStyle, scale: f32) -> FilterChain {
    let mut chain = chain(&style.get_effects().backdrop_filter.0, scale);
    chain.extend(effect(style, zgui_scene::property::BACKDROP_FILTER, scale));
    chain
}

/// The application effect `property` names, as far as how far it reads.
///
/// The parameters are deliberately absent: this conversion exists to answer how far a fragment
/// reads, which is the effect's own declaration and nothing else. What the effect is *drawn* with
/// is resolved where it is emitted, against a scene this stage does not have.
fn effect(style: &ComputedStyle, property: &str, scale: f32) -> Option<Filter> {
    let name = zgui_css::values::custom::text(style, property)?.trim();
    if name.is_empty() || name == "none" {
        return None;
    }
    let declared = zgui_scene::shader_named(name)?;
    if declared.mode != zgui_scene::ShaderMode::Filter {
        return None;
    }
    Some(Filter::Custom {
        shader: declared.id,
        params: zgui_scene::ShaderParamsSlot(0),
        reach: declared.reach * scale,
    })
}

/// Whether either chain reaches beyond the rectangle it is applied to.
///
/// This is the question the read-extent registry is a list of the answers to: a chain of per-pixel
/// filters, or no chain at all, reads exactly what it writes and never needs to be found again.
pub fn reads_outside(style: &ComputedStyle, scale: f32) -> bool {
    own(style, scale).iter().any(|it| !it.is_per_pixel())
        || backdrop(style, scale).iter().any(|it| !it.is_per_pixel())
        || !style.get_effects().backdrop_filter.0.is_empty()
        // A backdrop reads what is beneath it whatever its chain does, and an application's is no
        // different: the copy it samples is a region of the composite, not of anything it wrote.
        || zgui_css::values::custom::text(style, zgui_scene::property::BACKDROP_FILTER)
            .is_some_and(|name| !name.trim().is_empty())
}

/// One computed list, converted entry by entry.
///
/// Lengths arrive in CSS pixels and leave in device pixels, because everything a fragment carries
/// is measured on the physical grid.
fn chain(values: &[FilterValue], scale: f32) -> FilterChain {
    values
        .iter()
        .filter_map(|value| convert(value, scale))
        .collect()
}

/// One computed filter function.
fn convert(value: &FilterValue, scale: f32) -> Option<Filter> {
    Some(match value {
        FilterValue::Blur(radius) => Filter::Blur(deviation(radius.0.px() * scale)),
        FilterValue::Brightness(amount) => Filter::Brightness(amount.0),
        FilterValue::Contrast(amount) => Filter::Contrast(amount.0),
        FilterValue::Grayscale(amount) => Filter::Grayscale(amount.0),
        FilterValue::HueRotate(angle) => Filter::HueRotate(angle.radians()),
        FilterValue::Invert(amount) => Filter::Invert(amount.0),
        FilterValue::Opacity(amount) => Filter::Opacity(amount.0),
        FilterValue::Saturate(amount) => Filter::Saturate(amount.0),
        FilterValue::Sepia(amount) => Filter::Sepia(amount.0),
        FilterValue::DropShadow(shadow) => Filter::DropShadow {
            offset_x: shadow.horizontal.px() * scale,
            offset_y: shadow.vertical.px() * scale,
            blur: deviation(shadow.blur.0.px() * scale),
            color: [0.0; 4],
        },
        // A url() filter names a document-defined filter this engine does not resolve, and a
        // filter it cannot resolve has no kernel it can report.
        FilterValue::Url(_) => return None,
    })
}

/// The standard deviation a CSS blur radius means.
///
/// `blur(r)` is a Gaussian of standard deviation `r / 2`, which is the definition the filter
/// specification gives and the one every engine's blur radii were authored against.
fn deviation(radius: f32) -> f32 {
    radius.max(0.0) / 2.0
}

#[cfg(test)]
mod tests {
    use zgui_scene::Filter;

    use super::deviation;

    #[test]
    fn a_blur_radius_is_twice_its_standard_deviation() {
        assert_eq!(deviation(8.0), 4.0);
        // A negative radius cannot be written, and a blur of zero reaches nowhere.
        assert_eq!(deviation(-1.0), 0.0);
        assert!(Filter::Blur(deviation(0.0)).is_per_pixel());
        assert!(!Filter::Blur(deviation(8.0)).is_per_pixel());
    }
}
