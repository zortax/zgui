//! The surface a submenu opens onto.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::{Align, Placement, Side};

use crate::menu::content::MenuContentProps;
use crate::menu::sub::SubIntent;
use crate::overlay::OverlayState;

/// The items a submenu holds.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {DropdownMenuContent {
///         MenuSub {
///             MenuSubTrigger {"Export as"}
///             MenuSubContent {MenuItem {"PDF"}MenuItem {"CSV"}}
///         }
///     }}
/// }
/// # }
/// ```
#[component]
pub fn MenuSubContent(
    /// Where it is asked to go, before the window's edges have their say.
    ///
    /// Beside its trigger and lined up with its top, which is what a submenu looks like — and if
    /// there is no room to the right, the positioner crosses to the left on its own.
    #[prop(into, default = Signal::stored_local(Placement::new(Side::Right, Align::Start)))]
    placement: Signal<Placement, LocalStorage>,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let arrived = SubIntent::current();
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-menu--sub"), true)
        .listener(
            events::POINTER_ENTER,
            zgui::vocab::ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::PointerEnter>| {
                // The pointer made it across. The trigger armed a close when the pointer left it
                // without proof of intent, and a close that survives the arrival shuts the
                // submenu under a pointer that is inside it.
                if let Some(intent) = &arrived {
                    intent.closing().cancel();
                }
            },
        )
        .listener(
            events::KEY_DOWN,
            zgui::vocab::ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::KeyDown>| {
                if ev.key == Key::Named(NamedKey::ArrowLeft) {
                    // Back out of the branch rather than out of the whole menu: the left arrow is
                    // the submenu's own, and Escape is what closes everything.
                    ev.prevent_default();
                    ev.stop_propagation();
                    state.close();
                    state.trigger().focus();
                }
            },
        );

    view! {
        MenuContent(
            state = state,
            placement = placement,
            offset = {-4.0},
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
        }
    }
}
