//! One icon, drawn eight ways, to find out which of them draws anything.
//!
//! The gallery showed a set of icons that produce boxes of the right size and no ink at all, beside
//! another set of the same icons that draw normally. This is the difference between the two, taken
//! apart one property at a time: whether the icon carries an accessibility name, whether it sits in
//! something that sets a colour, and whether that colour is set on the icon itself.
//!
//! Run it with `cargo run -p zgui-ui --release --example icon_matrix`.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_icons::set::mark::{CHECK, CROSS, PLUS};
use zgui_ui_icons::set::status::{ALERT_TRIANGLE, INFO};
use zgui_ui_icons::set::ui::{ELLIPSIS, SEARCH};

/// The eight cases, each labelled with what it is.
#[component]
fn Matrix() -> impl IntoView {
    view! {
        column(class = "page") {
            row(class = "case") {
                text {"1 bare, no name"}
                Icon(icon = CHECK)
            }
            row(class = "case") {
                text {"2 bare, named"}
                Icon(icon = CHECK, label = "Check")
            }
            row(class = "case") {
                text {"3 colour on the icon, no name"}
                Icon(icon = CHECK, class = "inked")
            }
            row(class = "case") {
                text {"4 colour on the icon, named"}
                Icon(icon = CHECK, class = "inked", label = "Check")
            }
            row(class = "case inked") {
                text {"5 colour on the parent, no name"}
                Icon(icon = CHECK)
            }
            row(class = "case inked") {
                text {"6 colour on the parent, named"}
                Icon(icon = CHECK, label = "Check")
            }
            row(class = "case") {
                text {"7 in a button, no name"}
                Button(size = ButtonSize::Icon) {Icon(icon = CHECK)}
            }
            row(class = "case") {
                text {"8 in a button, named"}
                Button(size = ButtonSize::Icon, a11y:label = "Add") {Icon(icon = CHECK, label = "Check")}
            }
            column(class = "case") {
                text {"9 several, in a wrapping row"}
                row(class = "items") {
                    Icon(icon = CHECK, label = "A")
                    Icon(icon = CROSS, label = "B")
                    Icon(icon = PLUS, label = "C")
                    Icon(icon = CHEVRON_RIGHT, label = "D")
                    Icon(icon = SEARCH, label = "E")
                    Icon(icon = ELLIPSIS, label = "F")
                    Icon(icon = INFO, label = "G")
                    Icon(icon = ALERT_TRIANGLE, label = "H")
                }
            }
            column(class = "case") {
                text {"10 the same eight, unnamed"}
                row(class = "items") {
                    Icon(icon = CHECK)
                    Icon(icon = CROSS)
                    Icon(icon = PLUS)
                    Icon(icon = CHEVRON_RIGHT)
                    Icon(icon = SEARCH)
                    Icon(icon = ELLIPSIS)
                    Icon(icon = INFO)
                    Icon(icon = ALERT_TRIANGLE)
                }
            }
        }
    }
}

/// Opens the window.
fn main() -> Result<(), zgui::Error> {
    let id = std::env::var("ZGUI_PROBE_APPID").unwrap_or_else(|_| "zgui-icons".to_owned());
    app()
        .with_application_id(id.as_str())
        .with_title(id.as_str())
        .with_size(520.0, 420.0)
        .with_stylesheet(zgui::css!(
            ":root { background-color: #ffffff; color: #101010; font-family: sans-serif; }
             .page { padding: 16px; gap: 12px; }
             .case { gap: 12px; align-items: center; }
             .inked { color: #d81b60; }
             .items { gap: 8px; align-items: center; flex-wrap: wrap; }"
        ))
        .run(|| view! { Matrix() })
}
