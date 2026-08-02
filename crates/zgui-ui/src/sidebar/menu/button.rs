//! The control inside one entry of a sidebar's menu.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect};
use zgui::vocab::{AriaCurrent, UiState};
use zgui::{component, view};
use zgui_ui_primitives::popper::{Align, Placement, Side};

use crate::sidebar::context::SidebarContext;
use crate::sidebar::menu::state::SidebarMenuItemState;
use crate::sidebar::shape::{SidebarMenuSize, SidebarMenuVariant, SidebarSide};
use crate::sidebar::style;
use crate::tooltip::{TooltipContentProps, TooltipProps, TooltipTriggerProps};

/// The control inside a [`SidebarMenuItem`](crate::SidebarMenuItem).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarMenu {
///         SidebarMenuItem {
///             SidebarMenuButton(active = true, tooltip = "Files", on:click = move |_| ()) {
///                 "Files"
///             }
///         }
///     }}}}}
/// }
/// # }
/// ```
///
/// `active` is the place the user is at, announced as the current page and selectable in a sheet
/// through `data-active`. It is not the same as focus, and a panel that used the focus ring to say
/// where you are would forget it the moment you clicked into the document.
///
/// # Folded to icons
///
/// The control squares up to 32px and keeps its padding, so whatever it leads with stays exactly
/// where it was and everything after it is clipped away. That is why nothing here fades: the label
/// does not dim, it leaves.
///
/// A control with no label showing is a control with nothing to read, which is what `tooltip` is
/// for. The tip is raised on the panel's outward side, and only while the panel is folded — with
/// the label in plain sight a tip repeating it is noise.
#[component]
pub fn SidebarMenuButton(
    /// Whether this entry is the place being shown.
    #[prop(into, default = Signal::stored_local(false))]
    active: Signal<bool, LocalStorage>,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// How tall it is.
    #[prop(default = SidebarMenuSize::Default)]
    size: SidebarMenuSize,
    /// How it is drawn.
    #[prop(default = SidebarMenuVariant::Default)]
    variant: SidebarMenuVariant,
    /// What to say beside it while the panel is folded to icons.
    #[prop(into, optional)]
    tooltip: Option<String>,
    /// What it is called, for a reader, when what it holds does not say.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the control's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the entry reads as.
    children: Children,
) -> impl IntoView {
    style::install();
    let entry = SidebarMenuItemState::current();
    if let Some(entry) = entry {
        entry.take_size(size);
    }
    let watching = RenderEffect::new(move |_| {
        if let Some(entry) = entry {
            entry.take_active(active.get());
        }
    });
    on_cleanup_local(move || drop(watching));

    let mut semantics = A11yBinding::new(Role::Link)
        .disabled(move || disabled.get())
        .current(move || {
            if active.get() {
                AriaCurrent::Page
            } else {
                AriaCurrent::False
            }
        });
    let spoken = label.clone().or_else(|| tooltip.clone());
    if let Some(text) = spoken {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-active"), move || {
            active.get().then(|| "true".to_owned())
        })
        .attribute(zgui::view::AttrName::new("data-size"), size.name())
        .attribute(zgui::view::AttrName::new("data-variant"), variant.name())
        .state(UiState::CHECKED, move || active.get())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    let control = view! {
        control(
            class = "zui-sidebar__menu-button",
            tabindex = {Focus::Sequential},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    };

    let Some(tip) = tooltip.map(Signal::stored_local) else {
        return AnyView::new(control);
    };

    let context = SidebarContext::current();
    let outward = context
        .map(SidebarContext::side)
        .is_none_or(|side| side == SidebarSide::Left);
    let placement = Signal::derive_local(move || {
        Placement::new(
            if outward { Side::Right } else { Side::Left },
            Align::Center,
        )
    });

    let tipped = view! {
        Tooltip {
            TooltipTrigger(class = "zui-sidebar__menu-tip") {{control}}
            if move || context.is_some_and(SidebarContext::is_icon_only) {
                TooltipContent(placement = placement) {{move || tip.get()}}
            }
        }
    };
    AnyView::new(tipped)
}
