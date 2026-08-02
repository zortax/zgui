//! The twenty-one SVG paint longhands, none of which exists in this build.
//!
//! They are declared here rather than beside a reader because there is no reader to declare them
//! beside: the engine generates the whole group only for another target, so the parser does not
//! know the names and a style sheet using one loses that declaration without a word. Vector content
//! is painted from the element's own `color` together with custom properties instead, which is why
//! nothing in this framework is waiting on them.
//!
//! An SVG document drawn as content is not an exception. Its own `fill`, `stroke`, `stroke-width`
//! and the rest are read by the document reader out of the document, and never travel through this
//! cascade at all — the document's paint is the document's, and the element's `color` is what its
//! `currentColor` resolves against. So these names being absent costs a drawn document nothing.

use crate::parity::support::{AbsentReason, Support};

crate::register_properties! {
    clip_rule                   => Support::Absent(AbsentReason::GeckoOnly),
    color_interpolation         => Support::Absent(AbsentReason::GeckoOnly),
    color_interpolation_filters => Support::Absent(AbsentReason::GeckoOnly),
    fill                        => Support::Absent(AbsentReason::GeckoOnly),
    fill_opacity                => Support::Absent(AbsentReason::GeckoOnly),
    fill_rule                   => Support::Absent(AbsentReason::GeckoOnly),
    marker_end                  => Support::Absent(AbsentReason::GeckoOnly),
    marker_mid                  => Support::Absent(AbsentReason::GeckoOnly),
    marker_start                => Support::Absent(AbsentReason::GeckoOnly),
    paint_order                 => Support::Absent(AbsentReason::GeckoOnly),
    shape_rendering             => Support::Absent(AbsentReason::GeckoOnly),
    stroke                      => Support::Absent(AbsentReason::GeckoOnly),
    stroke_dasharray            => Support::Absent(AbsentReason::GeckoOnly),
    stroke_dashoffset           => Support::Absent(AbsentReason::GeckoOnly),
    stroke_linecap              => Support::Absent(AbsentReason::GeckoOnly),
    stroke_linejoin             => Support::Absent(AbsentReason::GeckoOnly),
    stroke_miterlimit           => Support::Absent(AbsentReason::GeckoOnly),
    stroke_opacity              => Support::Absent(AbsentReason::GeckoOnly),
    stroke_width                => Support::Absent(AbsentReason::GeckoOnly),
    text_anchor                 => Support::Absent(AbsentReason::GeckoOnly),
    _moz_context_properties     => Support::Absent(AbsentReason::GeckoOnly),
}

#[cfg(test)]
mod tests {
    use super::REGISTERED;

    #[test]
    fn the_group_is_twenty_one_longhands() {
        assert_eq!(REGISTERED.len(), 21);
    }

    #[test]
    fn the_vendor_prefixed_one_keeps_its_leading_hyphen() {
        let prefixed = REGISTERED
            .iter()
            .find(|row| row.ident() == "_moz_context_properties")
            .expect("the group holds it");
        assert_eq!(prefixed.css_name(), "-moz-context-properties");
    }
}
