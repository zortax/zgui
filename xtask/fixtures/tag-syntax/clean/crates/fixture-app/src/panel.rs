//! A panel, written the one way there is to write a view.

use zgui::prelude::*;

/// Describes the panel.
pub fn panel() -> impl IntoView {
    view! {
        row(class = "panel") {
            text {"Total"}
        }
    }
}
