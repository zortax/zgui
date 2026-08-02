//! Messages that appear in a corner, stack up and go away again.

mod item;
mod message;
mod queue;
mod style;

pub use crate::toast::item::{ToastItem, ToastItemProps};
pub use crate::toast::message::{Toast, ToastAction, ToastKind};
pub use crate::toast::queue::{Queued, ToastId, ToastQueue};
pub use crate::toast::style::ToastStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the toaster's rules are installed under.
pub(crate) const SHEET: &str = "zui-toast";

/// Which corner the toasts stack in.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ToastCorner {
    /// The bottom trailing corner.
    #[default]
    BottomRight,
    /// The bottom leading corner.
    BottomLeft,
    /// The top trailing corner.
    TopRight,
    /// The top leading corner.
    TopLeft,
}

impl ToastCorner {
    /// How this is written as an attribute value, which is what a style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BottomRight => "bottom-right",
            Self::BottomLeft => "bottom-left",
            Self::TopRight => "top-right",
            Self::TopLeft => "top-left",
        }
    }
}

/// The queue an enclosing [`Toaster`] published, when there is one.
///
/// `None` outside a toaster, which is an ordinary answer rather than a mistake: a component that
/// would announce something is perfectly usable in an application that has decided not to show
/// announcements, and asking is how it finds that out without panicking.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::toast::{Toast, use_toaster};
///
/// /// A button four components below the toaster that announces something anyway.
/// #[component]
/// fn Save() -> impl IntoView {
///     let toasts = use_toaster();
///     view! {
///         Button(on:click = move |_| {
///             if let Some(toasts) = toasts {
///                 toasts.push(Toast::new("Saved").description("Your changes are on the server."));
///             }
///         }) {
///             "Save"
///         }
///     }
/// }
/// ```
#[must_use]
pub fn use_toaster() -> Option<ToastQueue> {
    ToastQueue::current()
}

/// Somewhere for an application's announcements to go, and the queue behind them.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::toast::{Toast, ToastCorner, use_toaster};
///
/// /// An application with somewhere for its announcements to go.
/// #[component]
/// fn App() -> impl IntoView {
///     view! {
///         Toaster(corner = ToastCorner::BottomRight, limit = 3) {
///             Announce()
///         }
///     }
/// }
///
/// /// Anything inside the toaster can announce something.
/// #[component]
/// fn Announce() -> impl IntoView {
///     let toasts = use_toaster();
///     view! {
///         Button(on:click = move |_| {
///             if let Some(toasts) = toasts {
///                 toasts.push(Toast::new("Saved"));
///             }
///         }) {"Save"}
///     }
/// }
/// ```
///
/// # It wraps the application rather than sitting beside it
///
/// The queue reaches its callers through the scope tree, and a scope tree only goes downwards: a
/// toaster written as a sibling of the interface would publish a queue nothing in that interface
/// could find. So it is written once around the root, everything inside reaches the queue with
/// [`use_toaster`], and nothing in between has to know it might be announced through — which is the
/// whole point. A save button four components deep should not need a callback prop threaded through
/// four component signatures to say "Saved".
///
/// The stack itself is still portalled onto the toast band, so wrapping the interface costs the
/// layout one box and nothing else.
///
/// # Where the stack goes
///
/// On the toast band — the topmost — through a portal, so nothing the toaster happens to be
/// written inside can clip it, and nothing else paints over it. Which corner is a prop; the region has no size of its own, and each
/// toast is placed against that corner by [`ToastItem`], clear of whatever the ones between it and
/// the corner measured.
///
/// # What happens beyond the limit
///
/// The oldest toasts are asked to leave, one for each that is over — with the same exit as one the
/// reader dismissed, rather than blinking out. The newest are the ones kept, because a message that
/// has just arrived is the one somebody is waiting for.
///
/// # What a reader is told
///
/// Each toast is an alert; an error interrupts and everything else waits for a pause — see
/// [`ToastKind::live`]. The region itself is a status region, so a reader that missed one can go
/// and find it.
#[component]
pub fn Toaster(
    /// Which corner the toasts stack in.
    #[prop(default = ToastCorner::BottomRight)]
    corner: ToastCorner,
    /// How many may be showing at once.
    #[prop(default = 3)]
    limit: usize,
    /// What the region is called, for a reader.
    #[prop(into, default = String::from("Notifications"))]
    label: String,
    /// What the control that dismisses a toast is called, for a reader.
    #[prop(into, default = String::from("Dismiss"))]
    dismiss_label: String,
    /// Classes merged after the region's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The interface that announces things.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ToastStyle::CSS);
    let queue = ToastQueue::new(limit);
    provide_local_context(queue);

    // Stored, because the region is built inside a portal's own closure and what the caller handed
    // in has to survive that closure running more than once.
    let class = StoredValue::new_local(class);
    let attrs = StoredValue::new_local(attrs);
    let label = StoredValue::new_local(label);
    let dismiss_label = StoredValue::new_local(dismiss_label);

    // Whether the pointer is on the region, so the count the stack is held by is given back
    // exactly once however the pointer leaves. The region is the one element covering the whole
    // stack — toasts and the gaps between them alike — which is what stops the deck collapsing
    // under a pointer that is merely crossing from one toast to the next.
    let under = std::rc::Rc::new(std::cell::Cell::new(false));
    let entered = {
        let under = std::rc::Rc::clone(&under);
        move || {
            if !under.replace(true) {
                queue.hold();
            }
        }
    };
    let left = {
        let under = std::rc::Rc::clone(&under);
        move || {
            if under.replace(false) {
                queue.let_go();
            }
        }
    };
    // A region unmounted from under the pointer is never left, so the hold is given back with the
    // scope instead — the same guard makes the two paths safe to race.
    {
        let left = left.clone();
        on_cleanup_local(left);
    }

    view! {
        box(class = "zui-toast__host") {
            {children.into_view_once()}
            // On the toast band rather than the default popover one: an announcement has to stay
            // readable over whatever dialog happens to be open when it arrives.
            Portal(layer = {OverlayLayer::Toast}) {
            box(
                class = "zui-toast__region",
                class = ToastStyle::CLASS,
                attr:data-corner = corner.name(),
                // Collapsed into a deck while nobody is looking at it, and fanned out the moment
                // the pointer arrives. The same hold that stops the deadlines is what opens it,
                // because they are one question: is somebody reading this.
                attr:data-expanded = move || Some(queue.is_held().to_string()),
                // The region's own size, so it has a box for the pointer to be on: its children
                // are all absolutely placed, and a box sized by them alone would be a line of no
                // height that no pointer can enter.
                style:--zui-toast-extent = move || Some(format!("{}px", queue.extent())),
                on:pointer_enter = {
                    let entered = entered.clone();
                    move |_| entered()
                },
                on:pointer_leave = {
                    let left = left.clone();
                    move |_| left()
                },
                {..Attrs::new().a11y_from(
                    A11yBinding::new(Role::Status)
                        .label(move || zgui::vocab::SharedString::from(label.get_value()))
                        .live(zgui::vocab::Live::Polite),
                )},
                {..attrs.get_value()},
                class = class.get_value()
            ) {
                for queued in move || queue.showing(), key = |queued: &Queued| queued.id {
                    ToastItem(dismiss_label = dismiss_label.get_value(), queued = queued)
                }
            }
            }
        }
    }
}
