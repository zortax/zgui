//! Keeping content mounted until its exit animation has actually finished.

mod deadline;
mod state;
mod watch;

use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal};
use zgui::view::TimeoutHandle;
use zgui::{component, view};

use crate::diag::note;

pub use crate::presence::state::{PresenceContext, PresenceState, use_presence};
pub use crate::presence::watch::Listening;

/// Keeps its children mounted through their exit animation, and takes them away when it ends.
///
/// Unmounting a dialog the moment it closes deletes the fade-out along with it. Guessing a
/// duration instead — "it's a 200ms fade, so unmount in 200ms" — is a number in Rust that has to
/// agree with a number in CSS, and the two drift the first time anyone changes the sheet.
///
/// So nothing is guessed. The state goes to `closed`, the cascade starts whatever the style sheet
/// says happens at `[data-state="closed"]`, and the content is unmounted **when that animation
/// ends** — or immediately, in the same frame, when there is no animation to wait for.
///
/// # When the exit never ends
///
/// Waiting on an animation means waiting on an event, and an event can fail to arrive: a rule that
/// stopped matching mid-flight, an end delivered to an element that is no longer the one being
/// watched. Content that waited for ever would keep a modal surface's scrim over the window and
/// its focus trap around a subtree nobody can see, which is a window that answers nothing at all.
///
/// So an exit that has not finished within a second finishes anyway, and the content goes. The
/// animation is not the authority on whether a dismissal happens — only on how long it is allowed
/// to look good.
///
/// # What the content has to do
///
/// One thing: bind the state to an attribute, so the cascade has something to match on.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, css, view};
/// use zgui_ui_primitives::prelude::*;
///
/// #[component]
/// fn Popover(open: Signal<bool, zgui::reactive::LocalStorage>) -> impl IntoView {
///     let surface = NodeRef::new();
///     view! {
///         Presence(present = open, surface = surface) {
///             PresenceSurface(element_ref = surface)
///         }
///     }
/// }
///
/// #[component]
/// fn PresenceSurface(element_ref: NodeRef) -> impl IntoView {
///     let presence = use_presence();
///     view! {
///         surface(
///             class = "popover",
///             node_ref = element_ref,
///             attr:data-state = move || presence.map(|presence| presence.state_name().to_owned())
///         ) {
///             "content"
///         }
///     }
/// }
///
/// const SHEET: &str = css!(
///     ".popover { transition: opacity 180ms ease }
///      .popover[data-state=\"closed\"] { opacity: 0 }"
/// );
/// ```
///
/// # Why the element is the caller's
///
/// A wrapper of its own would be a box in the layout that the component author did not write, and
/// the animation belongs on the surface itself rather than on something around it. So the caller
/// hands over the handle of the element that animates, and this watches that one.
#[component]
pub fn Presence(
    /// Whether the content should be there.
    #[prop(into)]
    present: Signal<bool, LocalStorage>,
    /// The element whose exit animation decides when the content actually leaves.
    ///
    /// The caller's, not one of this component's own: the animation belongs on the surface itself
    /// rather than on a wrapper around it, and a wrapper would put a box in the layout that the
    /// component author never wrote.
    surface: NodeRef,
    /// What is kept mounted.
    children: ChildrenFn,
) -> impl IntoView {
    let who = crate::diag::next_id();
    let mounted = RwSignal::new_local(present.get_untracked());
    let state = RwSignal::new_local(if present.get_untracked() {
        PresenceState::Open
    } else {
        PresenceState::Closed
    });
    provide_local_context(PresenceContext::new(state.into()));
    note!(
        "presence.built",
        "who={who} present={} mounted={}",
        present.get_untracked(),
        mounted.get_untracked()
    );

    // The two timers an exit installs, held because dropping a handle cancels the timer it names.
    // The first is the one-frame check that catches an exit with no animation at all; the second
    // is the deadline that finishes one whose end never arrives. See [`deadline`].
    let pending: Rc<RefCell<Option<TimeoutHandle>>> = Rc::new(RefCell::new(None));
    let overdue: Rc<RefCell<Option<TimeoutHandle>>> = Rc::new(RefCell::new(None));

    let finish = move |what: &str, at: Option<zgui::view::NodeId>| {
        // Asked again rather than counted: a second animation may have started while the first was
        // running, and an exit that unmounted on the first end would cut the rest of it off.
        let leaving = state.get_untracked().is_leaving();
        let running = surface.running_animations();
        note!(
            "presence.end",
            "who={who} what={what} at={at:?} surface={:?} leaving={leaving} running={running} \
             mounted={}",
            surface.get_untracked(),
            mounted.get_untracked()
        );
        if leaving && running == 0 {
            mounted.set(false);
            note!("presence.unmount", "who={who} by={what}");
        }
    };

    // The listeners on the caller's element — on whichever element that handle names *now*. The
    // content is unmounted and built again every time it comes and goes, so the element under the
    // handle is a different one each time, and a set attached on the first open would be listening
    // to a departed node from the second open onwards.
    let attached = Listening::named(who, surface, move || {
        let mut held = Vec::new();
        // Both ends of both kinds. A cancelled animation is an animation that will produce no
        // end, and content that waited for one would stay mounted for ever.
        held.extend(surface.listen(
            events::ANIMATION_END,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::AnimationEnd>| {
                finish("animation-end", Some(ev.target));
            },
        ));
        held.extend(surface.listen(
            events::ANIMATION_CANCEL,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::AnimationCancel>| {
                finish("animation-cancel", Some(ev.target));
            },
        ));
        held.extend(surface.listen(
            events::TRANSITION_END,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::TransitionEnd>| {
                finish("transition-end", Some(ev.target));
            },
        ));
        held.extend(surface.listen(
            events::TRANSITION_CANCEL,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::TransitionCancel>| {
                finish("transition-cancel", Some(ev.target));
            },
        ));
        held
    });

    let watching = {
        let pending = Rc::clone(&pending);
        let overdue = Rc::clone(&overdue);
        RenderEffect::new(move |_| {
            if present.get() {
                note!(
                    "presence.open",
                    "who={who} was-mounted={} surface={:?}",
                    mounted.get_untracked(),
                    surface.get_untracked()
                );
                // Whatever the last exit installed is cancelled here rather than left to expire
                // harmlessly: content that closed and opened again inside a second would meet a
                // deadline armed for the exit before this entrance.
                pending.borrow_mut().take();
                overdue.borrow_mut().take();
                mounted.set(true);
                state.set(PresenceState::Open);
                return;
            }
            state.set(PresenceState::Closed);
            note!(
                "presence.close",
                "who={who} mounted={} surface={:?} running={}",
                mounted.get_untracked(),
                surface.get_untracked(),
                surface.running_animations()
            );
            if !mounted.get_untracked() {
                return;
            }
            // Deferred by one frame, and by exactly one. The attribute this effect just wrote has
            // not been cascaded yet, so nothing has started: asking now would always answer "no
            // animation" and every exit would be cut short. A zero-length timer runs at the start
            // of the next frame, after the cascade that started whatever the sheet asked for.
            //
            // The handle is kept, because dropping one cancels the timer — a fire-and-forget call
            // here would schedule a check that never runs and leave the content mounted for ever.
            *pending.borrow_mut() = Some(set_timeout(Duration::ZERO, move || {
                let leaving = state.get_untracked().is_leaving();
                let running = surface.running_animations();
                note!(
                    "presence.deferred",
                    "who={who} leaving={leaving} running={running} surface={:?} mounted={}",
                    surface.get_untracked(),
                    mounted.get_untracked()
                );
                if leaving && running == 0 {
                    mounted.set(false);
                    note!("presence.unmount", "who={who} by=deferred");
                }
            }));
            // And the deadline, which is what makes a dismissal that has been asked for happen
            // whatever the animations do. Every unmount above is driven by an event, and an event
            // that never arrives leaves a modal surface over a window that then answers nothing at
            // all — a consequence out of all proportion to one dropped animation end.
            *overdue.borrow_mut() = Some(deadline::arm(move || {
                if !state.get_untracked().is_leaving() || !mounted.get_untracked() {
                    return;
                }
                note!(
                    "presence.overdue",
                    "who={who} running={} surface={:?}",
                    surface.running_animations(),
                    surface.get_untracked()
                );
                mounted.set(false);
                note!("presence.unmount", "who={who} by=overdue");
            }));
        })
    };

    on_cleanup_local(move || {
        note!("presence.dropped", "who={who}");
        drop(attached);
        drop(watching);
        pending.borrow_mut().take();
        overdue.borrow_mut().take();
    });

    view! {
        if move || mounted.get() {
            {children.view()}
        } else {}
    }
}
