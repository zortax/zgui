//! Which CSS longhands this crate reads the value of.
//!
//! A parity claim is a count, and a count needs one declaration per property beside the code that
//! reads it. This crate reads more properties than any other — a box's size, its position, its
//! spacing, the two formatting contexts' own vocabularies, and everything the fragment pass turns
//! into geometry — so the declarations are grouped the way the readers are.
//!
//! | Module | What reads them |
//! |---|---|
//! | [`core`] | the properties every layout mode reads: sizes, insets, spacing, overflow |
//! | [`flex`] | the flexbox vocabulary |
//! | [`grid`] | the grid vocabulary |
//! | [`fragment`] | what the fragment pass turns into geometry, clips, transforms and ink |
//!
//! One row lives outside all four, beside the code that reads it: `text-overflow`, in
//! [`inline::ellipsis`](crate::inline::ellipsis). It is not a group of properties and putting it in
//! a group of one would be filing rather than declaring.
//!
//! ```
//! use zgui_css::parity::Registry;
//!
//! zgui_css::enable_css_features();
//! let mut registry = Registry::new();
//! registry.extend(&zgui_layout::parity::registered()).expect("no row declared twice");
//! assert!(registry.check().is_empty(), "every row still matches what the engine says");
//! assert!(registry.counts().implemented > 50);
//! ```

pub mod core;
pub mod flex;
pub mod fragment;
pub mod grid;

use zgui_css::parity::Registration;

/// Every longhand this crate declares, from all four groups.
pub fn registered() -> Vec<Registration> {
    [
        core::REGISTERED,
        flex::REGISTERED,
        grid::REGISTERED,
        fragment::REGISTERED,
        crate::inline::ellipsis::REGISTERED,
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
