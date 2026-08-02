//! Which CSS longhands this crate actually reads the value of.
//!
//! A parity claim is a count, and a count needs one declaration per property in the module that
//! reads it. Most of this crate's work is about *identities* rather than values — a repaint is
//! decided by comparing the addresses of whole groups of computed values, and a group's address
//! says nothing about which properties are in it — so the list below is short on purpose. It names
//! the properties whose value this crate destructures, and nothing else.
//!
//! ```
//! use zgui_css::parity::Registry;
//!
//! // Against the engine as this framework configures it: several of these are generated but
//! // switched off until the feature flags are set.
//! zgui_css::enable_css_features();
//!
//! let mut registry = Registry::new();
//! registry.extend(zgui_style::parity::REGISTERED).expect("no row declared twice");
//! assert!(registry.counts().implemented > 0);
//! assert!(registry.check().is_empty(), "every row still matches what the engine says");
//! ```

//! Every row is a claim about the engine *as this framework configures it*, which is why the test
//! below turns the feature flags on first: several of these properties are generated but switched
//! off until that runs, and a row checked against the unconfigured engine would describe a build
//! nobody ships.

use zgui_css::parity::Support;

zgui_css::register_properties! {
    font_family => Support::Implemented("zgui-style::device::metrics"),
    font_size => Support::Implemented("zgui-style::device::metrics"),
    font_weight => Support::Implemented("zgui-style::device::metrics"),
    font_style => Support::Implemented("zgui-style::device::metrics"),
    font_stretch => Support::Implemented("zgui-style::device::metrics"),
    font_variation_settings => Support::Implemented("zgui-style::device::metrics"),
    font_language_override => Support::Implemented("zgui-style::device::metrics"),
    line_height => Support::Implemented("zgui-style::engine::stylist"),
    visibility => Support::Implemented("zgui-style::damage::a11y_key"),
    direction => Support::Implemented("zgui-style::damage::a11y_key"),
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::Registry;

    #[test]
    fn every_declaration_still_matches_what_the_engine_says() {
        // Against the engine as this framework configures it, which is what the rows describe.
        zgui_css::enable_css_features();
        let mut registry = Registry::new();
        registry
            .extend(super::REGISTERED)
            .expect("no row is declared twice");
        assert_eq!(registry.len(), super::REGISTERED.len());
        assert!(
            registry.check().is_empty(),
            "a declaration here contradicts the engine as it is built: {:?}",
            registry.check()
        );
    }

    #[test]
    fn the_classified_count_is_not_zero() {
        zgui_css::enable_css_features();
        let mut registry = Registry::new();
        registry.extend(super::REGISTERED).expect("no row twice");
        assert!(registry.counts().implemented >= 10);
    }
}
