//! A surface that takes the window over until it is answered.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::overlay::SHEET;
use crate::overlay::content::OverlaySurfaceProps;
use crate::overlay::elevation::{ElevatedProps, SurfaceElevation};
use crate::overlay::lock::use_scroll_lock;
use crate::overlay::state::OverlayState;
use crate::overlay::style::OverlayStyle;

/// Everything a dialog, an alert dialog, a sheet and a drawer have in common: a scrim over the
/// window, a surface above it, focus confined to that surface and put back afterwards, and the
/// window stopped from scrolling behind it.
///
/// The scrim is a **sibling** of the dismissable layer rather than a wrapper around it, and that
/// is not a layout detail: a press on it has to count as a press *outside* the surface, which is
/// the whole of "click the backdrop to close".
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::overlay::{ModalSurface, ModalSurfaceProps, OverlayState};
///
/// /// A panel that has to be answered.
/// #[component]
/// fn Confirm() -> impl IntoView {
///     let state = OverlayState::uncontrolled(false, None).provide();
///     view! {
///         box {
///             control(node_ref = {state.trigger()}, on:click = move |_| state.open()) {"Delete"}
///             ModalSurface(state = state, role = Role::AlertDialog) {
///                 text {"This cannot be undone."}
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn ModalSurface(
    /// The overlay this is the surface of.
    state: OverlayState,
    /// What kind of surface it is, for a reader.
    #[prop(default = Role::Dialog)]
    role: Role,
    /// Whether a press on the scrim closes it.
    ///
    /// False for an alert dialog, which exists precisely because the answer matters: a stray press
    /// past a destructive confirmation must not count as an answer.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether Escape closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// Whether the scrim is drawn at all.
    #[prop(default = true)]
    scrim: bool,
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
    // A modal surface is on the modal band unless it was itself opened from something higher, and
    // everything opened inside it is raised to whatever it ended up on.
    let at = SurfaceElevation::raise(OverlayLayer::Modal);
    at.publish();
    let open = state.open_signal();
    use_scroll_lock(open);
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
                    if move || scrim {
                        Scrim()
                    } else {}
                    DismissableLayer(
                        layer = {OverlayLayer::Modal},
                        class = "zui-overlay-layer",
                        on_dismiss = dismiss,
                        dismiss_on_outside_press = dismiss_on_outside_press,
                        dismiss_on_escape = dismiss_on_escape
                    ) {
                        FocusScope(options = {FocusTrapOptions::MODAL}, class = "zui-overlay-scope") {
                            OverlaySurface(
                                state = state,
                                role = role,
                                modal = true,
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

/// What dims the window behind a modal surface.
///
/// # What it covers
///
/// **The whole window, the scrollbar's own strip included.** That is the rule, and it is stronger
/// than the one an ordinary fixed box follows. A fixed box's containing block is the *viewport*,
/// which is the window less whatever gutter the page reserved for its scrollbar, so a scrim sized as
/// a fraction of it stops fifteen pixels short of the right edge and of the bottom one and leaves
/// those two strips lit. Nothing about that reads as a scrollbar beside a dimmed page; it reads as a
/// scrim with a gap at the edge, which is exactly how it gets reported. So the scrim is sized in
/// viewport units — `100vw` by `100vh`, which are the window's own dimensions — and the bar under it
/// is dimmed along with everything else it is over.
///
/// A page with nothing to scroll reserves no gutter and its viewport *is* its window, so the rule
/// costs that page nothing and describes the same rectangle either way.
///
/// # Why it is a component of its own
///
/// The state it reports is the enclosing [`Presence`](zgui_ui_primitives::Presence)'s, and a context
/// is only reachable from below the scope that published it — so a scrim written inline in
/// [`ModalSurface`] would vanish on the frame the dialog closed and take its own fade with it.
#[component]
pub fn Scrim(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, OverlayStyle::CSS);
    let presence = use_presence();
    let data_state = move || {
        Some(
            presence
                .map_or("open", |presence| presence.state_name())
                .to_owned(),
        )
    };
    view! {
        box(
            class = {OverlayStyle::CLASS},
            class = "zui-overlay-scrim",
            attr:data-state = data_state,
            a11y:hidden = true,
            {..attrs},
            class = class
        )
    }
}
