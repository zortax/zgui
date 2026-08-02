//! One toast on the stack: where it sits, how long it stays, and what takes it away.

mod exit;
mod expiry;
mod swipe;


use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CROSS;

use crate::toast::SHEET;
use crate::toast::item::exit::Departure;
use crate::toast::item::expiry::Expiry;
use crate::toast::item::swipe::{LetGo, Swipe};
use crate::toast::queue::{Queued, ToastQueue};
use crate::toast::style::ToastStyle;

/// One toast on a [`Toaster`](crate::Toaster)'s stack.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Toaster {text {"the interface"}} }
/// # }
/// ```
///
/// Written as its own component because each toast owns a deadline, a gesture and an exit, and all
/// three have to go away exactly when the toast does — which is what a scope per toast gives and a
/// loop inside one component does not.
///
/// # Two elements, and why
///
/// A slot, and the toast inside it. The slot is placed absolutely against the corner and carries the
/// distance from it as a transform, which is a property a transition can move: that is what makes the
/// toasts above a departing one slide down into the gap instead of appearing in it. The toast itself
/// carries only its entrance and its exit, as keyframes.
///
/// The split is not decoration. A transition and a keyframe animation that move the same property on
/// the same element are two authorities over one value, and the frame the animation lets go of it is
/// a frame nobody has defined. Here neither element has both.
///
/// # How far it is from the corner
///
/// Whatever the toasts between it and the corner measured, which reaches the sheet as
/// `--zui-toast-offset`. Nothing is written down: a toast with a description is taller than one
/// without, and a stack stepped by a fixed amount would put one through another the first time a
/// caller wrote a longer message.
///
/// # The deadline
///
/// Cancelled while the pointer is anywhere on the stack, not merely on this toast: reading the second
/// of three messages must not let the first and third disappear from under it. The gaps between the
/// toasts count as on it — the hold is the region's, whose box covers the whole outline — so
/// walking up the stack never drops the hold half-way across one. Moving off starts the wait again
/// from the beginning.
///
/// # The swipe
///
/// The pointer is taken over once the gesture has travelled far enough to be a gesture, and not
/// before — because a toast that captured the pointer on the press would be handed the release that
/// its own close button was waiting for. How far it has gone reaches the sheet as
/// `--zui-toast-swipe`, so the toast follows the finger; letting go past the threshold dismisses it
/// and letting go short of it puts it back.
#[component]
pub fn ToastItem(
    /// The toast, and the name its queue gave it.
    queued: Queued,
    /// What the control that dismisses it is called, for a reader.
    #[prop(into, default = String::from("Dismiss"))]
    dismiss_label: String,
    /// Classes merged after the toast's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, ToastStyle::CSS);
    let queue = ToastQueue::current();
    let id = queued.id;
    let toast = queued.toast;
    let kind = toast.what();

    let dismiss = move || {
        if let Some(queue) = queue {
            queue.dismiss(id);
        }
    };
    let leaving = move || queue.is_some_and(|queue| queue.is_leaving(id));

    // The slot is what is measured, because what the toast above needs to clear is this one's whole
    // box: the toast, and the gap the slot carries as padding on the side the next one is on. The
    // *content* size, though, and not the border box: the box observation answers where the slot is
    // being drawn, through whatever transform is moving it, and a collapsed deck scales its slots —
    // so a border-box reading shrinks a little on every frame of the collapse, each shrink rewrites
    // the queue, each rewrite rebuilds the rows, and each rebuild starts the movement again. The
    // content size is the layout's own answer, which no transform touches; the gap the slot carries
    // as padding is added back from the same constant the region's extent is computed with.
    let slot = NodeRef::new();
    let surface = NodeRef::new();
    let measured = slot.observe_content_size();
    let reporting = RenderEffect::new(move |_| {
        let Some(queue) = queue else {
            return;
        };
        let content = measured.get();
        if content.height.0 <= 0.0 {
            // Not laid out yet, which the queue reads as "pushes nothing" — reported as such
            // rather than as a bare gap with no toast in it.
            return;
        }
        let scale = if slot.scale() > 0.0 {
            slot.scale()
        } else {
            1.0
        };
        queue.measure(id, content.height.0 / scale + crate::toast::queue::place::GAP);
    });

    let expiry = Expiry::new(toast.stays_for(), dismiss);
    let waiting = {
        let expiry = expiry.clone();
        RenderEffect::new(move |_| {
            let held = queue.is_some_and(ToastQueue::is_held);
            if held || leaving() {
                expiry.stop();
            } else {
                expiry.start();
            }
        })
    };

    let departure = Departure::new(queue, id, surface);
    let departing = departure.watch(leaving);
    let finished = {
        let departure = departure.clone();
        move || departure.settle()
    };

    {
        let expiry = expiry.clone();
        on_cleanup_local(move || {
            expiry.stop();
            drop(reporting);
            drop(waiting);
            drop(departing);
        });
    }

    let swipe = Swipe::new();
    let depth = move || queue.map_or(0, |queue| queue.depth_of(id));
    let place = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-swiping"), move || {
            swipe.is_swiping().then(|| "true".to_owned())
        })
        // Whether this is the one on top of the stack. A collapsed stack shows only its front
        // toast's contents, and the ones behind it are the shapes peeking out from under it.
        .attribute(zgui::view::AttrName::new("data-front"), move || {
            Some((depth() == 0).to_string())
        })
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-toast-offset"),
            move || {
                Some(format!(
                    "{}px",
                    queue.map_or(0.0, |queue| queue.offset_of(id))
                ))
            },
        )
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-toast-depth"),
            move || Some(depth().to_string()),
        )
        // Where this slot paints among the others. The region's document order is newest first,
        // which alone would paint the oldest toast over everything in front of it.
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-toast-layer"),
            move || Some(queue.map_or(0, |queue| queue.layer_of(id)).to_string()),
        )
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-toast-swipe"),
            move || Some(format!("{}px", swipe.distance())),
        );

    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-kind"), kind.name())
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if leaving() { "closed" } else { "open" }.to_owned())
        })
        .a11y_from(
            A11yBinding::new(Role::Alert)
                .live(kind.live())
                .label(toast.title().to_owned()),
        );

    let body = toast
        .body()
        .map(|text| view! { text(class = "zui-toast__description") {{text.to_owned()}} });
    let mark = kind.mark().map(
        |icon| view! { box(class = "zui-toast__icon", a11y:hidden = true) { Icon(icon = icon) } },
    );

    // The two buttons a message can carry. Built from what the message holds rather than from props,
    // because the message is written where something happened and read where the stack is drawn.
    let action = toast.action_button().cloned().map(|button| {
        let label = button.label().to_owned();
        AnyView::new(view! {
            control(
                class = "zui-toast__action",
                tabindex = {Focus::Sequential},
                on:click = move |_| {
                    button.run();
                    dismiss();
                },
                {..Attrs::new().a11y_from(A11yBinding::new(Role::Button).label(label.clone()))}
            ) {
                {label}
            }
        })
    });
    let cancel = toast.cancel_button().cloned().map(|button| {
        let label = button.label().to_owned();
        AnyView::new(view! {
            control(
                class = "zui-toast__cancel",
                tabindex = {Focus::Sequential},
                on:click = move |_| {
                    button.run();
                    dismiss();
                },
                {..Attrs::new().a11y_from(A11yBinding::new(Role::Button).label(label.clone()))}
            ) {
                {label}
            }
        })
    });

    view! {
        // No pointer handlers here: the hold belongs to the region, which is the one element
        // covering the whole stack — see the region's own comment in `toast/mod.rs`. A slot that
        // held it instead would also have to take the pointer, and a slot's gap padding lies over
        // the toast behind it, right where that toast's own close control is pressed.
        box(
            class = "zui-toast__slot",
            node_ref = slot,
            {..place}
        ) {
            box(
                class = "zui-toast",
                node_ref = surface,
                on:pointer_down = move |ev| swipe.press(ev.position.x.0),
                on:pointer_move = move |ev| {
                    if swipe.moved(ev.position.x.0) {
                        ev.capture_pointer();
                    }
                },
                on:pointer_up = move |ev| {
                    match swipe.let_go() {
                        LetGo::Dismiss => {
                            ev.release_pointer();
                            dismiss();
                        }
                        LetGo::Restore => ev.release_pointer(),
                        LetGo::Nothing => {}
                    }
                },
                on:pointer_cancel = move |ev| {
                    ev.release_pointer();
                    swipe.cancel();
                },
                // Whichever animation it was: the row is only taken away when the toast is leaving
                // and nothing on it is still running, so an entrance that happens to end here
                // answers for itself.
                on:animation_end = move |_| finished(),
                {..own},
                {..attrs},
                class = class
            ) {
                {mark}
                column(class = "zui-toast__text") {
                    text(class = "zui-toast__title") {{toast.title().to_owned()}}
                    {body}
                }
                {cancel}
                {action}
                control(
                    class = "zui-toast__close",
                    tabindex = {Focus::Sequential},
                    on:click = move |_| dismiss(),
                    {..Attrs::new().a11y_from(A11yBinding::new(Role::Button).label(dismiss_label))}
                ) {
                    Icon(icon = CROSS, size = {IconSize::Sm})
                }
            }
        }
    }
}
