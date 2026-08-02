//! The control that folds a sidebar away.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::ui::PANEL_LEFT;

use crate::sidebar::context::SidebarContext;
use crate::sidebar::style;

/// The control that folds a [`Sidebar`](crate::Sidebar) away and brings it back.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {
///         Sidebar()
///         SidebarInset {SidebarTrigger(label = "Toggle the panel")}
///     }
/// }
/// # }
/// ```
///
/// It says whether the panel is expanded and names the panel it controls, so a reader who presses
/// it is told what happened rather than being left to notice.
///
/// The same thing happens on <kbd>Ctrl</kbd>+<kbd>B</kbd> from anywhere in the window, which the
/// control announces as its shortcut.
///
/// # One mark, not two
///
/// The mark is a panel, and it does not change when the panel folds: it names the thing being
/// operated rather than the direction it is about to travel, so a user who has learnt where the
/// control is does not have to re-read it every time they press it.
#[component]
pub fn SidebarTrigger(
    /// What the control is called, for a reader.
    #[prop(into, default = String::from("Toggle sidebar"))]
    label: String,
    /// Classes merged after the control's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    style::install();
    let context = SidebarContext::current();
    let open = move || context.is_some_and(SidebarContext::is_open);

    let mut semantics = A11yBinding::new(Role::Button)
        .label(label)
        .keyboard_shortcut("Ctrl+B")
        .expanded(open);
    if let Some(context) = context {
        semantics = semantics.controls(context.panel());
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if open() { "expanded" } else { "collapsed" }.to_owned())
        })
        .a11y_from(semantics);

    view! {
        control(
            class = "zui-sidebar__trigger",
            tabindex = {Focus::Sequential},
            on:click = move |_| { if let Some(context) = context { context.toggle() } },
            {..own},
            {..attrs},
            class = class
        ) {
            Icon(icon = PANEL_LEFT, size = IconSize::Md)
        }
    }
}
