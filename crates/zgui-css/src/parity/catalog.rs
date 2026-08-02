//! Every property name the style engine generated, which is the denominator a parity count needs.
//!
//! A count of implemented CSS properties is meaningless without the set it is counted against, and
//! that set is not a list anyone can write down: it is whatever the engine, configured the way this
//! framework configures it, actually generated. The engine writes that list out while it builds,
//! and the table here is that list, read at build time.
//!
//! # Aliases are names, not properties
//!
//! The list holds vendor-prefixed spellings beside canonical ones — `-webkit-transform-origin` is
//! a name the parser accepts, and it is a name an author may write, so a parity denominator that
//! dropped it would flatter itself. It is not a separate property, though: it resolves to the same
//! longhand, so it is classified by whatever classifies its target.
//! [`Longhand::canonical`] is that resolution, answered by the engine rather than by a table.
//!
//! A prefixed spelling is a longhand alias only when the property it aliases is itself a longhand.
//! `-webkit-transform` aliases the `transform` shorthand, so it appears under [`shorthands`]
//! instead, and resolves to no longhand at all.
//!
//! ```
//! use zgui_css::parity::catalog;
//!
//! let all = catalog::longhands();
//! assert!(all.len() > 300, "the engine generates a few hundred longhands");
//!
//! let prefixed = all
//!     .iter()
//!     .find(|longhand| longhand.css_name == "-webkit-transform-origin")
//!     .expect("the engine accepts the prefixed spelling");
//! assert_eq!(prefixed.canonical().as_deref(), Some("transform-origin"));
//! assert!(prefixed.is_alias());
//! ```

use style::properties::{NonCustomPropertyId, PropertyId};

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// One property name the engine generated, and the preference gating it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Longhand {
    /// The name as a style sheet writes it.
    pub css_name: &'static str,
    /// The preference that has to be on before a style sheet may use it.
    ///
    /// `None` means the property is always available. The framework's own bootstrap turns on every
    /// preference it targets, so a gated property is usually reachable in practice — but the gate
    /// is what a parity report has to name when it is not.
    pub pref: Option<&'static str>,
}

impl Longhand {
    /// The longhand this name resolves to, which for a canonical name is itself.
    ///
    /// Answered by the engine, so a name whose alias target moved cannot go stale here.
    pub fn canonical(&self) -> Option<String> {
        let id = PropertyId::parse_unchecked(self.css_name, None).ok()?;
        let longhand = id.longhand_id()?;
        Some(NonCustomPropertyId::from(longhand).name().to_owned())
    }

    /// Whether this name is a second spelling of some other longhand.
    pub fn is_alias(&self) -> bool {
        self.canonical().is_some_and(|name| name != self.css_name)
    }
}

/// Every longhand name the engine generated, in the order it lists them.
pub fn longhands() -> Vec<Longhand> {
    LONGHAND_TABLE
        .iter()
        .map(|(css_name, pref)| Longhand {
            css_name,
            pref: *pref,
        })
        .collect()
}

/// Every shorthand name the engine generated, on the same terms.
///
/// A register of longhands never mentions these, and that is exactly why they are worth publishing:
/// a declaration that names one is malformed, and telling "malformed" apart from "unknown property"
/// needs this list.
pub fn shorthands() -> Vec<Longhand> {
    SHORTHAND_TABLE
        .iter()
        .map(|(css_name, pref)| Longhand {
            css_name,
            pref: *pref,
        })
        .collect()
}

/// Every distinct longhand the generated names resolve to, sorted and deduplicated.
///
/// This is the set a register has one row per. It is smaller than [`longhands`] by exactly the
/// number of alias spellings.
pub fn canonical_longhands() -> Vec<String> {
    let mut names: Vec<String> = longhands().iter().filter_map(Longhand::canonical).collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::{canonical_longhands, longhands, shorthands};

    /// The table is what the engine generated, so it cannot be empty and cannot be tiny.
    ///
    /// Without this the whole instrument would pass while measuring nothing: a denominator that
    /// silently read as empty makes every property classified and every parity count perfect.
    #[test]
    fn the_denominator_is_the_engines_own_property_list() {
        assert!(longhands().len() > 300, "{}", longhands().len());
        assert!(shorthands().len() > 50, "{}", shorthands().len());
    }

    /// Alias spellings collapse onto their targets, and canonical ones onto themselves.
    #[test]
    fn every_name_resolves_to_a_longhand() {
        for longhand in longhands() {
            assert!(
                longhand.canonical().is_some(),
                "`{}` is listed as a longhand but does not resolve to one",
                longhand.css_name,
            );
        }
        assert!(canonical_longhands().len() < longhands().len());
    }

    /// A shorthand is not a longhand, which is what makes a register row naming one malformed.
    #[test]
    fn no_shorthand_resolves_to_a_longhand() {
        for shorthand in shorthands() {
            assert_eq!(
                shorthand.canonical(),
                None,
                "`{}` is listed as a shorthand",
                shorthand.css_name,
            );
        }
    }

    /// Some of the generated names are gated, and a report that could not name the gate would be
    /// reporting the wrong reason for the silence an author sees.
    #[test]
    fn the_gated_names_carry_the_preference_that_gates_them() {
        let gated: Vec<&str> = longhands()
            .iter()
            .filter_map(|longhand| longhand.pref)
            .collect();
        assert!(!gated.is_empty());
        assert!(gated.iter().all(|pref| pref.contains('.')));
    }
}
