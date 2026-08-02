//! A surface that floats beside the control that opened it.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::overlay::SHEET;
use crate::overlay::content::OverlaySurfaceProps;
use crate::overlay::elevation::{ElevatedProps, SurfaceElevation};
use crate::overlay::state::OverlayState;
use crate::overlay::style::OverlayStyle;

/// Everything a popover, a menu, a tooltip and a select's list have in common: portalled out of
/// the tree, kept mounted through its exit animation, dismissed when it is the topmost thing open,
/// and placed against its trigger.
///
/// Four behaviours in one order, and the order is the whole of it. The portal is outermost so the
/// surface escapes whatever clipped or transformed ancestor its trigger lives in. The presence is
/// next so the exit animation runs before anything is unmounted. The dismissable layer is inside
/// that so it is registered exactly as long as the surface is on screen. The positioner is
/// innermost, because it has to measure a surface that already exists.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::overlay::{AnchoredSurface, AnchoredSurfaceProps, OverlayState};
///
/// /// A note that floats above whatever opened it.
/// #[component]
/// fn Note() -> impl IntoView {
///     let state = OverlayState::uncontrolled(false, None).provide();
///     view! {
///         box {
///             control(node_ref = {state.trigger()}, on:click = move |_| state.toggle()) {"Why?"}
///             AnchoredSurface(state = state, role = Role::Dialog) {
///                 text {"Because."}
///             }
///         }
///     }
/// }
/// ```
///
/// # What a style sheet gets
///
/// `data-state` on the surface, for the enter and exit animation, and `data-side` and `data-align`
/// on the positioner around it, for which way it actually went. An arrow and a slide-in direction
/// are written from those and from nothing else.
#[component]
pub fn AnchoredSurface(
    /// The overlay this is the surface of.
    state: OverlayState,
    /// The lowest overlay band it goes on, which is what decides what is above what.
    ///
    /// A floor rather than an answer: a surface opened from inside another rises to that one's
    /// band, so a select written inside a dialog is drawn above the dialog rather than under it.
    /// See [`SurfaceElevation`](crate::overlay::SurfaceElevation).
    #[prop(default = OverlayLayer::Popover)]
    layer: OverlayLayer,
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::BOTTOM))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// What kind of surface it is, for a reader.
    #[prop(default = Role::GenericContainer)]
    role: Role,
    /// How focus behaves while it is open, when it is confined at all.
    ///
    /// `None` leaves focus where it was, which is what a tooltip and a hover card want: they are
    /// not things to operate, and taking the caret off the control the user is on would be a
    /// surface that hijacks the keyboard for showing a definition.
    #[prop(optional)]
    trap: Option<FocusTrapOptions>,
    /// Whether a press past it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether Escape closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Classes merged after the surface's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the surface.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is on the surface.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, OverlayStyle::CSS);
    // Read before it is published, because what this surface is raised to is decided by whatever
    // it was opened from, and what it publishes is where it ended up.
    let at = SurfaceElevation::raise(layer);
    at.publish();
    let open = state.open_signal();
    let dismiss = UnsyncCallback::new(move |_reason: DismissReason| state.close());
    // Held rather than moved: the surface is rebuilt every time it re-opens, and a bundle a
    // closure moved out of on the first open is a bundle the second open does not have.
    let class = StoredValue::new_local(class);
    let attrs = StoredValue::new_local(attrs);
    let children = StoredValue::new_local(children);

    view! {
        Portal(layer = {at.layer()}) {
            Elevated(at = at) {
                Presence(present = open, surface = {state.content()}) {
                    DismissableLayer(
                        // One band for every dismissable surface in this library, so that which one
                        // answers a press is decided by the order they opened — which for a popover
                        // inside a dialog is the popover. That is a different question from the paint
                        // order the portal's band above settles, and the two now agree: a surface
                        // opened inside another is both drawn over it and dismissed before it.
                        layer = {OverlayLayer::Modal},
                        class = "zui-overlay-layer",
                        on_dismiss = dismiss,
                        exclude = {state.trigger()},
                        dismiss_on_outside_press = dismiss_on_outside_press,
                        dismiss_on_escape = dismiss_on_escape
                    ) {
                        Popper(
                            anchor = {state.trigger()},
                            placement = placement,
                            offset = offset,
                            class = "zui-overlay-positioner"
                        ) {
                            Confined(trap = trap) {
                                OverlaySurface(
                                    state = state,
                                    role = role,
                                    modal = {trap.is_some()},
                                    class = {class.get_value()},
                                    {..attrs.get_value()}
                                ) {
                                    {children.get_value().view()}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Confines focus to its children, or leaves them alone.
///
/// A [`FocusScope`] with no options is still a trap, so "no trap at all" cannot be said by leaving
/// its props out — and a tooltip that trapped focus would be a tooltip nobody can tab past. This
/// is the branch, written once.
#[component]
pub fn Confined(
    /// How focus behaves, when it is confined at all.
    trap: Option<FocusTrapOptions>,
    /// What is inside.
    children: ChildrenFn,
) -> impl IntoView {
    match trap {
        Some(options) => AnyView::new(view! {
            FocusScope(options = options, class = "zui-overlay-scope") {{children.view()}}
        }),
        None => children.view(),
    }
}
