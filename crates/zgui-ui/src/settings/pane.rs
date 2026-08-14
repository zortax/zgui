//! What one page holds.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::{component, view};

use crate::settings::context::SettingsContext;
use crate::settings::style;

/// What one page of a [`Settings`](crate::Settings) holds, shown while that page is chosen.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Settings(default_page = "appearance") {
///         SettingsPages {SettingsPage(value = "appearance") {"Appearance"}}
///         SettingsPane(value = "appearance") {
///             SettingsGroup {
///                 SettingsGroupLabel {"Theme"}
///                 SettingsItem(label = "Dark mode") {Switch()}
///             }
///         }
///     }
/// }
/// # }
/// ```
///
/// # Built each time, or kept
///
/// The pane's own element is always there — it is what the entry's `controls` relation names, and
/// a relation to something that comes and goes is a relation that is sometimes wrong. What comes
/// and goes is the *content*: by default it is built when the pane is shown and dropped when it is
/// hidden, so the page nobody is looking at costs no layout, no paint and no subscriptions.
///
/// Set `keep_mounted` where the content holds something a person would be sorry to lose: a
/// half-typed value that is not yet in a signal of the caller's, or a scroll position.
///
/// # Keyboard
///
/// The pane is itself a tab stop, so <kbd>Tab</kbd> from the page list lands on the pane rather
/// than skipping it.
#[component]
pub fn SettingsPane(
    /// Which entry shows this pane.
    #[prop(into)]
    value: String,
    /// Whether the content stays built while another pane is showing.
    #[prop(default = false)]
    keep_mounted: bool,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The groups.
    children: ChildrenFn,
) -> impl IntoView {
    style::install();
    let context = SettingsContext::current();
    let name = Rc::new(value);
    let node = context.map_or_else(NodeRef::new, |settings| settings.pane_of(&name));

    let selected = {
        let name = Rc::clone(&name);
        move || context.is_some_and(|settings| settings.is_selected(&name))
    };

    let mut semantics = A11yBinding::new(Role::TabPanel).hidden({
        let selected = selected.clone();
        move || !selected()
    });
    if let Some(settings) = context {
        semantics = semantics.labelled_by(settings.entry_of(&name));
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), {
            let selected = selected.clone();
            move || Some(if selected() { "active" } else { "inactive" }.to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-value"), {
            let name = Rc::clone(&name);
            move || Some(name.to_string())
        })
        .a11y_from(semantics);

    let inside = if keep_mounted {
        AnyView::new(children.view())
    } else {
        let shown = selected.clone();
        AnyView::new(view! { Show(when = shown) {{children.view()}} })
    };

    view! {
        column(
            class = "zui-settings__pane",
            node_ref = node,
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {inside}
        }
    }
}
