//! The list a select's options sit on.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue};
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_primitives::{Align, Placement, Side};

use crate::listbox::{Listbox, ListboxCatalogueOfProps};
use crate::overlay::{AnchoredSurfaceProps, OverlayState};
use crate::select::SHEET;
use crate::select::style::SelectStyle;

/// The options of a [`Select`](crate::Select), under its trigger.
///
/// Nothing here is focusable, and that is the design rather than an oversight: the caret stays on
/// the trigger the whole time the list is open, so the options are pointed at rather than visited.
///
/// # What a closed select still knows
///
/// While the list is closed the options are built once more, out of sight, purely so that they say
/// what their values read as. Without it the trigger would have nothing to ask — its options *are*
/// its list — and a select handed a value would show its placeholder over it until somebody had
/// opened the list and closed it again. The two are never mounted together, so nothing is on the
/// list twice.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Select {
///         SelectTrigger {SelectValue()}
///         SelectContent {SelectItem(value = "gbp") {"Pound"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn SelectContent(
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::new(Side::Bottom, Align::Start)))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the list.
    #[prop(attrs)]
    attrs: Attrs,
    /// The options.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, SelectStyle::CSS);
    let state = Listbox::current().map_or_else(
        || OverlayState::uncontrolled(false, None),
        |listbox| listbox.surface(),
    );
    let own = Attrs::new().class_toggle(ClassName::new("zui-select__list"), true);
    // Held rather than moved: the options are written once and built in two places, and a bundle
    // one closure moved out of is a bundle the other never has.
    let children = StoredValue::new_local(children);

    // Two views side by side rather than one inside a box of this component's own: the list is
    // portalled and contributes nothing where it is written, and a wrapper here would put a box in
    // the caller's layout — a gap between the trigger and whatever comes after it, from a component
    // that is supposed to occupy no room at all.
    let list = view! {
        AnchoredSurface(
            state = state,
            placement = placement,
            offset = offset,
            role = {Role::ListBox},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.get_value().view()}
        }
    };
    let described = view! {
        if move || !state.is_open() {
            ListboxCatalogueOf {{children.get_value().view()}}
        } else {}
    };
    (list.into_view(), described.into_view())
}
