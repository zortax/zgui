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

/// What writing one declaration into a block did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wrote {
    /// The value did not parse for that property, or spelled out more than that property. The
    /// block is left exactly as it was.
    Rejected,
    /// The block already said exactly this, so nothing was written.
    ///
    /// Distinguished from [`Wrote::Changed`] because the caller re-cascades on a change, and a
    /// re-cascade is a restyle, a relayout and a repaint of the element that took it. A view that
    /// re-states a length it is already showing — which is what every binding driven by a signal
    /// that recomputes more often than it changes does — would otherwise pay all three, on a box
    /// that may be very much larger than the part of it anybody can see.
    Unchanged,
    /// The declaration is now different from what it was.
    Changed,
}

/// Writes `property: value` into `block`, replacing any declaration already there for it.
pub(crate) fn set(
    block: &mut PropertyDeclarationBlock,
    name: &str,
    id: &PropertyId,
    value: &str,
) -> Wrote {
    let parsed = attribute(&format!("{name}:{value}"));
    if parsed.is_empty()
        || !parsed
            .declarations()
            .iter()
            .all(|declaration| declaration.id().is_or_is_longhand_of(id))
    {
        return Wrote::Rejected;
    }
    // An empty block declares nothing, so anything that parses is a change and neither side needs
    // serialising. Worth its own test because it is the case every first write takes, and the
    // comparison below is two allocations and two passes over the value.
    if !block.is_empty() {
        // Compared as the block itself would serialise them rather than against the caller's text,
        // so that `10.0px` and `10px` — the same declaration written two ways — compare equal.
        // Both sides are produced by the same serialiser from parsed values, which is what makes
        // that exact rather than approximate.
        //
        // A rejected parse has already returned, so the right-hand side is `Some`; a block that
        // does not declare the property answers `None` and compares unequal, which is a change.
        if serialised(block, id) == serialised(&parsed, id) {
            return Wrote::Unchanged;
        }
    }
    for declaration in parsed.declarations() {
        block.push(declaration.clone(), Importance::Normal);
    }
    Wrote::Changed
}

/// What `block` currently says `id` is, as CSS text.
///
/// `None` when it says nothing, which is not the same answer as the empty string: a property that
/// is absent and one declared as nothing are different states of the block.
fn serialised(block: &PropertyDeclarationBlock, id: &PropertyId) -> Option<String> {
    let mut text = String::new();
    block.property_value_to_css(id, &mut text).ok()?;
    if text.is_empty() { None } else { Some(text) }
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

    use super::{Wrote, attribute, property_id, remove, set};

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
        assert_eq!(set(&mut block, "color", &id, "red"), Wrote::Changed);
        assert_eq!(set(&mut block, "color", &id, "blue"), Wrote::Changed);
        assert_eq!(block.len(), 1);
        assert!(remove(&mut block, &id));
        assert_eq!(block.len(), 0);
        assert!(!remove(&mut block, &id));
    }

    #[test]
    fn a_value_that_does_not_parse_leaves_the_block_alone() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("color").expect("a real property");
        assert_eq!(set(&mut block, "color", &id, "red"), Wrote::Changed);
        assert_eq!(
            set(&mut block, "color", &id, "not-a-colour"),
            Wrote::Rejected,
        );
        assert_eq!(block.len(), 1);
    }

    /// A value is not a place to smuggle a second declaration in from.
    #[test]
    fn a_value_that_spells_out_another_property_is_refused_whole() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("color").expect("a real property");
        assert_eq!(
            set(&mut block, "color", &id, "red; display: none"),
            Wrote::Rejected,
        );
        assert_eq!(block.len(), 0);
    }

    /// The case a virtualised list hits on every frame of a glide: a binding re-states the length
    /// it is already showing, and turning that into a re-cascade costs a relayout and a repaint of
    /// a box as tall as the whole list.
    #[test]
    fn re_stating_a_declaration_the_block_already_holds_changes_nothing() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("padding-top").expect("a real property");
        assert_eq!(set(&mut block, "padding-top", &id, "24px"), Wrote::Changed);
        assert_eq!(
            set(&mut block, "padding-top", &id, "24px"),
            Wrote::Unchanged,
            "the same length again is not a change",
        );
        assert_eq!(
            set(&mut block, "padding-top", &id, "24.0px"),
            Wrote::Unchanged,
            "nor is the same length spelled differently: the comparison is of parsed values",
        );
        assert_eq!(
            set(&mut block, "padding-top", &id, "25px"),
            Wrote::Changed,
            "and a different length still is one",
        );
        assert_eq!(block.len(), 1);
    }

    /// A shorthand is unchanged only when every longhand it expands to is.
    #[test]
    fn a_shorthand_is_unchanged_only_when_all_of_it_is() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("padding").expect("a real property");
        assert_eq!(set(&mut block, "padding", &id, "1px 2px"), Wrote::Changed);
        assert_eq!(set(&mut block, "padding", &id, "1px 2px"), Wrote::Unchanged);
        assert_eq!(set(&mut block, "padding", &id, "1px 3px"), Wrote::Changed);
    }

    #[test]
    fn a_shorthand_expands_and_every_longhand_it_produced_belongs_to_it() {
        let mut block = PropertyDeclarationBlock::new();
        let id = property_id("padding").expect("a real property");
        assert_eq!(set(&mut block, "padding", &id, "1px 2px"), Wrote::Changed);
        assert_eq!(block.len(), 4);
        assert!(remove(&mut block, &id));
        assert_eq!(block.len(), 0);
    }
}
