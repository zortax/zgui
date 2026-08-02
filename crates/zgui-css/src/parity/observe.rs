//! Reading one property off a computed style by name, so a check can be written over all of them.
//!
//! Every ordinary reader of a computed style names the property it wants and gets a typed value.
//! An instrument that has to ask *"did the author write something here that nothing acted on"*
//! cannot do that: it is handed a property name at run time, one of a few hundred, and needs the
//! same answer for all of them. The engine can serialise any property's computed value, and that
//! serialisation is what makes the question askable at all.
//!
//! ```
//! use zgui_css::StyleDraft;
//! use zgui_css::parity::observe;
//!
//! let initial = StyleDraft::initial().build();
//! assert_eq!(observe::computed_value(&initial, "line-break").as_deref(), Some("auto"));
//!
//! // A name the parser does not know has no value to read.
//! assert_eq!(observe::computed_value(&initial, "fill-opacity"), None);
//! ```

use style::properties::{LonghandId, PropertyDeclarationId, PropertyId};

use crate::computed::style::ComputedStyle;

/// The computed value of one property of one style, serialised.
///
/// The name is spelled as a style sheet spells it, and an alias spelling answers for its target.
/// `None` means the name is not a longhand this build of the engine knows.
pub fn computed_value(style: &ComputedStyle, css_name: &str) -> Option<String> {
    Some(style.computed_value_to_string(PropertyDeclarationId::Longhand(longhand_id(css_name)?)))
}

/// Whether `style` carries something other than the initial value for `css_name`.
///
/// This is the question behind *"I wrote CSS and nothing happened"*: a property whose computed
/// value is the initial one was not written by anybody, so nothing was ignored. Comparison is
/// against a style built from initial values rather than against a table of initial values, so
/// there is no second copy of what the engine already knows.
pub fn differs_from_initial(
    style: &ComputedStyle,
    initial: &ComputedStyle,
    css_name: &str,
) -> bool {
    match (
        computed_value(style, css_name),
        computed_value(initial, css_name),
    ) {
        (Some(found), Some(expected)) => found != expected,
        _ => false,
    }
}

/// The engine's identifier for one longhand, or nothing when the name is not one.
fn longhand_id(css_name: &str) -> Option<LonghandId> {
    PropertyId::parse_unchecked(css_name, None)
        .ok()?
        .longhand_id()
}

#[cfg(test)]
mod tests {
    use zgui_geom::CssPx;

    use crate::computed::draft::StyleDraft;

    use super::{computed_value, differs_from_initial};

    /// Every longhand the engine generated can be read back off a style.
    ///
    /// Without this the lint built on top would be silently partial: a property whose serialisation
    /// answered nothing would never be reported, and the properties most likely to be unread are
    /// exactly the ones nobody has exercised.
    #[test]
    fn every_generated_longhand_serialises() {
        crate::prefs::enable_css_features();
        let style = StyleDraft::initial().build();
        for longhand in crate::parity::catalog::longhands() {
            assert!(
                computed_value(&style, longhand.css_name).is_some(),
                "`{}` is generated but has no readable computed value",
                longhand.css_name,
            );
        }
    }

    /// A style that differs is reported as differing, and one that does not is not.
    ///
    /// Both halves, because a comparison that always answered "the same" would keep the lint above
    /// it permanently silent and permanently green.
    #[test]
    fn a_changed_property_is_the_only_thing_reported_as_changed() {
        let initial = StyleDraft::initial().build();
        let larger = StyleDraft::initial().with_font_size(CssPx(32.0)).build();

        assert!(differs_from_initial(&larger, &initial, "font-size"));
        assert!(!differs_from_initial(&larger, &initial, "line-break"));
        assert!(!differs_from_initial(&initial, &initial, "font-size"));
    }
}
