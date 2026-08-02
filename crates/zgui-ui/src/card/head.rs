//! The heading of a card: its title, its description and the control beside them.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::{component, view};

use crate::card::SHEET;
use crate::card::style::CardStyle;

/// What a [`CardAction`] tells the heading it is inside.
///
/// The heading lays its pieces out on a grid, and whether that grid has a second column depends on
/// whether anything is going to be put in it. A sheet cannot ask that — the selector that would is
/// not one this engine matches — so the heading hands a signal down and the action sets it.
/// `Copy`, so an action stores one without cloning.
#[derive(Copy, Clone)]
pub struct CardHeaderContext {
    /// Whether an action is mounted inside this heading.
    action: RwSignal<bool, LocalStorage>,
}

impl CardHeaderContext {
    /// The heading the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether an action is mounted inside it.
    #[must_use]
    pub fn has_action(self) -> bool {
        self.action.get()
    }

    /// Says an action is mounted, and unsays it when the calling scope goes away.
    pub fn claim_action(self) {
        self.action.set(true);
        on_cleanup_local(move || {
            self.action.try_set(false);
        });
    }
}

/// The heading of a [`Card`](crate::Card): its title, its description and its action.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Card {CardHeader {CardTitle {"March"}}} }
/// # }
/// ```
///
/// The title and the description stack; a [`CardAction`] sits to the right of both of them, level
/// with the title. That is one cell spanning two rows, so the heading is a grid — and it grows its
/// second column only when there is an action to put in it.
#[component]
pub fn CardHeader(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    let context = CardHeaderContext {
        action: RwSignal::new_local(false),
    };
    provide_local_context(context);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-card__header"), true)
        .class_toggle(
            zgui::view::ClassName::new("zui-card__header--action"),
            move || context.has_action(),
        );

    view! {
        box({..own}, {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// What a [`Card`](crate::Card) is about, in one line.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Card {CardHeader {CardTitle {"March"}}} }
/// # }
/// ```
#[component]
pub fn CardTitle(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    view! {
        label(class = "zui-card__title", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// A line under a [`CardTitle`] qualifying it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Card {CardHeader {CardDescription {"Due soon"}}} }
/// # }
/// ```
#[component]
pub fn CardDescription(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    view! {
        box(class = "zui-card__description", {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// The control at the far end of a [`CardHeader`], level with its title.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::card::{CardAction, CardActionProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Card {
///         CardHeader {
///             CardTitle {"March"}
///             CardDescription {"Due on the 28th"}
///             CardAction {Button {"Pay"}}
///         }
///     }
/// }
/// # }
/// ```
///
/// Written inside the heading rather than after it, because that is where it belongs to a reader:
/// the control acts on what the title names. Where it is *drawn* is the heading's grid, which is
/// what puts it beside both lines instead of under them.
#[component]
pub fn CardAction(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The control.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CardStyle::CSS);
    if let Some(header) = CardHeaderContext::current() {
        header.claim_action();
    }

    view! {
        box(class = "zui-card__action", {..attrs}, class = class) {{children.into_view_once()}}
    }
}
