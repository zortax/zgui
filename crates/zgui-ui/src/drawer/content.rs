//! The panel a drawer raises, and the bar that says it can be pulled.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::{AttrName, ClassName};
use zgui::{component, view};

use crate::overlay::{ModalSurfaceProps, OverlayState, SurfaceLabels};
use crate::sheet::{SHEET_NAME, SheetSide, SheetStyle};

/// The drawer itself: a panel up from the bottom of the window, over a dimmed background.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Drawer {
///         DrawerTrigger {"Share"}
///         DrawerContent {DrawerTitle {"Share"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn DrawerContent(
    /// Which edge it comes in from.
    ///
    /// The bottom unless said otherwise, which is what a drawer usually is. The two edges that run
    /// the width of the window keep their inward corners rounded and centre their heading; the two
    /// down the sides are a [`Sheet`](crate::Sheet)'s shape with a drawer's handle.
    #[prop(default = SheetSide::Bottom)]
    direction: SheetSide,
    /// Whether it draws the grab handle at its top.
    #[prop(default = true)]
    handle: bool,
    /// Whether a press on the dimmed window behind it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether <kbd>Escape</kbd> closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Classes merged after the panel's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is in the drawer.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let labels = SurfaceLabels::current().unwrap_or_default();

    let own = Attrs::new()
        .class_toggle(ClassName::new(SheetStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-sheet"), true)
        .class_toggle(ClassName::new("zui-drawer"), true)
        .attribute(AttrName::new("data-side"), direction.name())
        .a11y_from(
            A11yBinding::unspecified()
                .labelled_by(labels.title())
                .described_by(labels.description()),
        );

    view! {
        ModalSurface(
            state = state,
            role = {Role::Dialog},
            dismiss_on_outside_press = dismiss_on_outside_press,
            dismiss_on_escape = dismiss_on_escape,
            {..own},
            {..attrs},
            class = class
        ) {
            if move || handle {
                DrawerHandle()
            } else {}
            {children.view()}
        }
    }
}

/// The short bar at the top of a drawer.
///
/// Decorative, and declared so: it is a picture of an affordance rather than a control, and a
/// reader that announced it would announce something there is nothing to do with.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Drawer {DrawerContent(handle = false) {DrawerHandle()}} }
/// # }
/// ```
#[component]
pub fn DrawerHandle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET_NAME, SheetStyle::CSS);
    view! { box(class = "zui-drawer__handle", a11y:hidden = true, {..attrs}, class = class) }
}
