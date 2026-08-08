//! What this build of the style engine supports, and what it does not.
//!
//! The panel somebody reaches for when a declaration they wrote did nothing. It answers the
//! question in the only form that is useful — *is this property implemented, is it parsed and
//! ignored, or is it out of reach in this build, and if it is out of reach what should I write
//! instead* — and it answers it from the register the crates themselves declare rather than from a
//! table maintained beside them.
//!
//! The declarations are gathered from the crates that make them, and the gathering is checked by
//! [`registrations`]'s own test rather than trusted: a source left out would leave longhands with
//! no declaration at all, which is exactly what
//! [`Registry::unclassified`](zgui_css::parity::Registry::unclassified) reports.

use zgui::prelude::*;
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the macro names the props type, the tag names the component"
)]
use crate::panel::frame::{Line, LineProps};

/// The parity tab.
#[component]
pub(crate) fn ParityPanel() -> impl IntoView {
    let counts = || {
        let mut registry = zgui_css::parity::Registry::new();
        let _ = registry.merge(&registrations());
        registry.counts()
    };
    view! {
        column(class = "zgui-devtools__body") {
            text(class = "zgui-devtools__head") {"CSS parity, as this build reports it"}
            Line(
                name = "longhands",
                value = move || zgui_css::parity::catalog::longhands().len().to_string()
            )
            Line(name = "implemented", value = move || counts().implemented.to_string())
            Line(name = "parsed and ignored", value = move || counts().ignored.to_string())
            Line(name = "absent", value = move || counts().absent.to_string())
            text(class = "zgui-devtools__head") {"out of reach, and what to write instead"}
            // By row and not by subject: two rows may describe the same property from two
            // different angles, and a keyed list whose keys collide drops one of them.
            for row in || zgui_css::parity::GAPS.iter().enumerate().collect::<Vec<_>>(),
                key = {|(at, _): &(usize, &zgui_css::parity::Gap)| *at}
            {
                row(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__key") {{row.1.subject}}
                    text(class = "zgui-devtools__value") {{row.1.instead}}
                }
                row(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__key") {""}
                    text(class = "zgui-devtools__value-quiet") {{row.1.reason}}
                }
            }
        }
    }
}

/// Every parity declaration this build carries.
///
/// One entry per source: the crates that read properties declare beside the code that reads them,
/// and the longhands nothing reads are declared beside the catalogue. A source missing from this
/// list is not a smaller number — it is a longhand with no declaration at all, which is what the
/// test below refuses.
fn registrations() -> Vec<zgui_css::parity::Registration> {
    [
        zgui_style::parity::REGISTERED.to_vec(),
        zgui_text_style::parity::REGISTERED.to_vec(),
        zgui_layout::parity::registered(),
        zgui_paint::parity::registered(),
        zgui_runtime::parity::REGISTERED.to_vec(),
        zgui_css::parity::gap::inherited_svg::REGISTERED.to_vec(),
        zgui_css::parity::backlog::registered(),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::registrations;

    /// The panel classifies every longhand the engine generates.
    ///
    /// This is the whole guard on gathering the declarations here: a forgotten source shows up as
    /// longhands nobody classified, not as a count that is quietly too low.
    #[test]
    fn every_longhand_the_engine_generates_is_classified() {
        zgui_css::enable_css_features();
        let mut registry = zgui_css::parity::Registry::new();
        registry.merge(&registrations());
        let canonical = zgui_css::parity::catalog::canonical_longhands();
        assert_eq!(
            registry.unclassified(canonical.iter().map(String::as_str)),
            Vec::<String>::new(),
            "a parity source is missing from this panel",
        );
    }
}
