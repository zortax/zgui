//! The longhands no crate has claimed, declared here so that none of them is *unclassified*.
//!
//! # Why a central list is the right answer for exactly these rows
//!
//! Declarations belong beside the code that reads a property, and a hand-maintained central table
//! is what that arrangement exists to avoid. These rows have no reader to sit beside, so this is
//! where they live: with the catalogue that supplies the denominator, rather than with any one
//! consumer. What keeps them from rotting is not discipline but arithmetic — a parity census
//! requires this list and the consuming crates' declarations to be **disjoint** and, together, to
//! cover every longhand the engine generates. Adding a reader for a property therefore forces its
//! row out of this list in the same change, and a longhand the engine starts or stops generating
//! fails until this list is edited.
//!
//! A second net sits under that one. Every row here says nothing reads the property, and a probe
//! contradicts a row that is wrong: a property declared unread whose value visibly moves the
//! fragment tree is an under-claim, and it fails.
//!
//! ```
//! use zgui_css::parity::{Registry, Support, backlog};
//!
//! zgui_css::enable_css_features();
//! let mut registry = Registry::new();
//! registry.extend(&backlog::registered()).expect("no row declared twice");
//! assert!(registry.iter().all(|row| matches!(row.support(), Support::Ignored(_))));
//! ```

pub mod geometry;
pub mod motion;
pub mod text;
pub mod visual;

use crate::parity::Registration;

/// Every unclaimed longhand.
pub fn registered() -> Vec<Registration> {
    [
        geometry::REGISTERED,
        visual::REGISTERED,
        text::REGISTERED,
        motion::REGISTERED,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use crate::parity::{Registry, Support};

    /// Every row here says the same thing: the property parses and nothing reads it.
    ///
    /// Whether that is *true* is answered where the readers are visible, because this crate cannot
    /// see them. What can be answered here is that the list is well formed — no property declared
    /// twice, and no row claiming something this module is not for.
    #[test]
    fn every_row_is_a_parsed_and_unread_declaration() {
        crate::enable_css_features();
        let mut registry = Registry::new();
        registry
            .extend(&super::registered())
            .expect("no row declared twice");
        assert!(!registry.is_empty(), "the backlog was read as empty");
        assert!(
            registry
                .iter()
                .all(|row| matches!(row.support(), Support::Ignored(_))),
        );
    }
}
