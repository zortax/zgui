//! Which CSS longhands this crate reads the value of.
//!
//! The declarations themselves live beside the code that reads them — a colour's row is in the
//! module that resolves the colour, a shadow's row is in the module that lowers the shadow. What
//! this module adds is the list of those groups, so that a caller assembling the whole framework's
//! declarations names one thing rather than nine, and so that a new lowering module whose rows are
//! left out of the census is a one-line omission a reviewer can see.
//!
//! ```
//! use zgui_css::parity::Registry;
//!
//! zgui_css::enable_css_features();
//! let mut registry = Registry::new();
//! registry.extend(&zgui_paint::parity::registered()).expect("no row declared twice");
//! assert!(registry.check().is_empty(), "every row still matches what the engine says");
//! assert!(registry.counts().implemented > 20);
//! ```

use zgui_css::parity::Registration;

/// Every longhand this crate declares, from all the groups that lower or emit one.
pub fn registered() -> Vec<Registration> {
    [
        crate::lower::REGISTERED,
        crate::lower::background::REGISTERED,
        crate::lower::border::REGISTERED,
        crate::lower::clip::REGISTERED,
        crate::lower::filter::REGISTERED,
        crate::lower::outline::REGISTERED,
        crate::lower::shadow::REGISTERED,
        crate::lower::transform::REGISTERED,
        crate::emit::text::REGISTERED,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::Registry;

    /// Every row is a claim about the engine, and the engine is asked whether it is still true.
    #[test]
    fn every_declaration_still_matches_what_the_engine_says() {
        zgui_css::enable_css_features();
        let mut registry = Registry::new();
        registry
            .extend(&super::registered())
            .expect("no row is declared twice");
        assert_eq!(registry.len(), super::registered().len());
        assert!(
            registry.check().is_empty(),
            "a declaration here contradicts the engine as it is built: {:?}",
            registry.check(),
        );
    }
}
