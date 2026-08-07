//! The panel a floating surface actually is.

use zgui::prelude::*;
use zgui::view::AttrName;
use zgui::{component, view};
use zgui_ui_primitives::use_presence;

use crate::overlay::SHEET;
use crate::overlay::state::OverlayState;
use crate::overlay::style::OverlayStyle;

/// The element a dialog, a menu, a popover and a tooltip all are: a panel that reports whether it
/// is arriving or leaving.
///
/// It exists as its own component rather than as an element inside
/// [`AnchoredSurface`](crate::overlay::AnchoredSurface) for one reason, and it is not tidiness: the
/// state it reports comes from the enclosing [`Presence`](zgui_ui_primitives::Presence), and a
/// context is only reachable from a scope *below* the one that published it. Written inline it
/// would read the state of whatever presence happened to be around the whole overlay, which
/// during an exit animation is none.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::overlay::{OverlayState, OverlaySurface, OverlaySurfaceProps};
///
/// /// A panel that fades out rather than vanishing.
/// #[component]
/// fn Panel() -> impl IntoView {
///     let state = OverlayState::uncontrolled(true, None);
///     view! {
///         OverlaySurface(state = state, role = Role::Dialog, modal = true) {
///             text {"Are you sure?"}
///         }
///     }
/// }
/// ```
#[component]
pub fn OverlaySurface(
    /// The overlay this is the surface of.
    state: OverlayState,
    /// What kind of surface it is, for a reader.
    #[prop(default = Role::GenericContainer)]
    role: Role,
    /// Whether it takes the interaction over, which is what makes a dialog modal to a reader as
    /// well as to the pointer.
    #[prop(default = false)]
    modal: bool,
    /// Classes merged after the surface's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is on the surface.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, OverlayStyle::CSS);
    // The presence, if there is one, is the authority: during an exit animation the overlay is
    // already closed and the surface is still on screen, and it is the surface's own state that
    // the exit keyframe hangs off.
    let presence = use_presence();
    let data_state = move || {
        Some(match presence {
            Some(presence) => presence.state_name().to_owned(),
            None => state.state_name().to_owned(),
        })
    };

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-surface"), true)
        .attribute(AttrName::new("data-state"), data_state)
        .a11y_from(
            A11yBinding::with_role(role)
                .modal(modal)
                // Which control this surface belongs to. Without it a reader that lands on a menu
                // has no way back to the button that opened it, and a surface portalled onto an
                // overlay band is nowhere near that button in the tree.
                .popup_for(state.trigger()),
        );

    view! {
        box(node_ref = {state.content()}, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
