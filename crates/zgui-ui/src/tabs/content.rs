//! One panel of a tab set.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::{component, view};

use crate::tabs::style::TabsStyle;
use crate::tabs::{SHEET, TabsContext};

/// One panel of a [`Tabs`](crate::Tabs), shown while its tab is the selected one.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Tabs(default_value = "profile") {
///         TabsList {TabsTrigger(value = "profile") {"Profile"}}
///         TabsContent(value = "profile", keep_mounted = true) {
///             text {"Your name and picture."}
///         }
///     }
/// }
/// # }
/// ```
///
/// # Built each time, or kept
///
/// The panel's own element is always there — it is what the tab's `controls` relation names, and a
/// relation to something that comes and going is a relation that is sometimes wrong. What comes
/// and goes is the *content*: by default it is built when the panel is shown and dropped when it
/// is hidden, so the panel nobody is looking at costs no layout, no paint and no subscriptions.
///
/// Set `keep_mounted` where the content holds something a user would be sorry to lose: a
/// half-filled form, a scroll position, a video that is playing.
///
/// # Keyboard
///
/// The panel is itself a tab stop, so <kbd>Tab</kbd> from the strip lands on the content rather
/// than skipping a panel that has nothing focusable in it.
#[component]
pub fn TabsContent(
    /// Which tab shows this panel.
    #[prop(into)]
    value: String,
    /// Whether the content stays built while another panel is showing.
    #[prop(default = false)]
    keep_mounted: bool,
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the panel holds.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, TabsStyle::CSS);
    let context = TabsContext::current();
    let name = Rc::new(value);
    let node = context.map_or_else(NodeRef::new, |tabs| tabs.panel_of(&name));

    let selected = {
        let name = Rc::clone(&name);
        move || context.is_some_and(|tabs| tabs.is_selected(&name))
    };

    let mut semantics = A11yBinding::new(Role::TabPanel).hidden({
        let selected = selected.clone();
        move || !selected()
    });
    if let Some(tabs) = context {
        semantics = semantics.labelled_by(tabs.trigger_of(&name));
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
        box(
            class = "zui-tabs__content",
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
