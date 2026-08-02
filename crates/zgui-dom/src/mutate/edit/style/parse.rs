//! Turning declaration text into declarations.
//!
//! The parser is error-recovering in the same way it is for a stylesheet: an unknown property or
//! an unparsable value is dropped and everything around it survives. Nothing here reports the drop
//! back through the sheet-level error reporter, because a declaration set on one element has no
//! source location to report — the caller learns from the return value instead, which is what lets
//! a view layer surface the mistake at the place it was written.
//!
//! One declaration is parsed by spelling it out and parsing that, rather than by handing the value
//! to a per-property entry point, and the result is then checked to be about the property that was
//! asked for. That check is load-bearing: without it `set("color", "red; display: none")` would
//! write a second declaration nobody asked for.

use std::str::FromStr;

use servo_arc::Arc as ServoArc;
use style::context::QuirksMode;
use style::properties::{Importance, PropertyDeclarationBlock, PropertyId, parse_style_attribute};
use style::stylesheets::{CssRuleType, UrlExtraData};

/// The base a declaration's URL-valued components resolve against.
///
/// Built by inference rather than by naming the URL library: the type is fixed by the field it
/// goes into, so nothing here has to name a crate the ledger does not permit.
fn base_url() -> UrlExtraData {
    fn parsed<T: FromStr>(text: &str) -> T
    where
        T::Err: core::fmt::Debug,
    {
        text.parse().expect("a well-formed base URL")
    }
    UrlExtraData(ServoArc::new(parsed("zgui:///")))
}

/// Parses a whole `style` attribute's worth of declarations.
///
/// Invalid declarations are dropped and the valid ones survive, exactly as they would in a
/// stylesheet.
pub(crate) fn attribute(css: &str) -> PropertyDeclarationBlock {
    parse_style_attribute(
        css,
        &base_url(),
        None,
        QuirksMode::NoQuirks,
        CssRuleType::Style,
    )
}

/// The property `name` denotes, if this build has one.
///
/// A name gated to another engine, and a name that is not a property at all, are the same answer
/// here: there is no declaration to make. Custom properties always resolve, because a custom
/// property is whatever an author called it.
pub(crate) fn property_id(name: &str) -> Option<PropertyId> {
    PropertyId::parse_enabled_for_all_content(name).ok()
}

/// Writes `property: value` into `block`, replacing any declaration already there for it.
///
/// Returns whether a declaration was made. `false` means the value did not parse for that
/// property, or spelled out more than that property, and the block is left exactly as it was.
pub(crate) fn set(
    block: &mut PropertyDeclarationBlock,
    name: &str,
    id: &PropertyId,
    value: &str,
) -> bool {
    let parsed = attribute(&format!("{name}:{value}"));
    if parsed.is_empty()
        || !parsed
            .declarations()
            .iter()
            .all(|declaration| declaration.id().is_or_is_longhand_of(id))
    {
        return false;
    }
    for declaration in parsed.declarations() {
        block.push(declaration.clone(), Importance::Normal);
    }
    true
}

/// Removes whatever `block` declares for `property`, and says whether it declared anything.
pub(crate) fn remove(block: &mut PropertyDeclarationBlock, id: &PropertyId) -> bool {
    let Some(first) = block.first_declaration_to_remove(id) else {
        return false;
    };
    block.remove_property(id, first);
    true
}

#[cfg(test)]
mod tests {
    use style::properties::PropertyDeclarationBlock;

    use super::{attribute, property_id, remove, set};

    #[test]
    fn an_attribute_keeps_the_declarations_that_parsed_and_drops_the_rest() {
        let block = attribute("color: red; nonsense: 3; display: flex");
        assert_eq!(block.len(), 2);
    }

    #[test]
    fn a_property_gated_to_another_engine_resolves_to_nothing() {
        assert!(property_id("color").is_some());
        assert!(property_id("--brand").is_some());
        assert!(property_id("fill").is_none());
        assert!(property_id("not-a-property").is_none());
    }

    #[test]
    fn setting_the_same_property_twice_replaces_rather_than_appends() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("color").expect("a real property");
        assert!(set(&mut block, "color", &id, "red"));
        assert!(set(&mut block, "color", &id, "blue"));
        assert_eq!(block.len(), 1);
        assert!(remove(&mut block, &id));
        assert_eq!(block.len(), 0);
        assert!(!remove(&mut block, &id));
    }

    #[test]
    fn a_value_that_does_not_parse_leaves_the_block_alone() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("color").expect("a real property");
        assert!(set(&mut block, "color", &id, "red"));
        assert!(!set(&mut block, "color", &id, "not-a-colour"));
        assert_eq!(block.len(), 1);
    }

    /// A value is not a place to smuggle a second declaration in from.
    #[test]
    fn a_value_that_spells_out_another_property_is_refused_whole() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("color").expect("a real property");
        assert!(!set(&mut block, "color", &id, "red; display: none"));
        assert_eq!(block.len(), 0);
    }

    #[test]
    fn a_shorthand_expands_and_every_longhand_it_produced_belongs_to_it() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("padding").expect("a real property");
        assert!(set(&mut block, "padding", &id, "1px 2px"));
        assert_eq!(block.len(), 4);
        assert!(remove(&mut block, &id));
        assert_eq!(block.len(), 0);
    }
}
