//! A surface that closes when something outside it is pressed, or Escape is held down.

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, UnsyncCallback};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

use crate::diag::note;
use crate::dismiss::stack::{LayerId, LayerStack};

/// Why a [`DismissableLayer`] was asked to close.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum DismissReason {
    /// Something outside the layer was pressed.
    OutsidePress,
    /// Escape was pressed.
    EscapeKey,
}

impl DismissReason {
    /// How this is written, for a transcript or a log.
    pub const fn name(self) -> &'static str {
        match self {
            Self::OutsidePress => "outside-press",
            Self::EscapeKey => "escape",
        }
    }
}

/// Asks to be closed when something outside it is pressed, or when Escape is pressed.
///
/// Every floating surface needs this and none of them should write it: hearing about a press
/// somewhere else in the window means listening on the window's root, deciding whether the press
/// was inside means asking the engine, and deciding whether the press was *yours* means knowing
/// what else is open.
///
/// # Nesting
///
/// Exactly one layer answers a press or an escape: the topmost one. A popover open inside a dialog
/// is dismissed by one press past it, and the dialog stays open — press again and the dialog goes.
/// Which layer is topmost is decided by the overlay band first and by the order they opened
/// second, so a toast raised before a dialog still answers before it.
///
/// A layer that has been asked to close is not open, and stops claiming Escape the moment it is
/// told to go — while it is still on the screen playing its exit, so it goes on claiming presses.
/// That is what makes two presses of Escape close two nested surfaces: the second arrives while
/// the first is still fading, and it belongs to the surface behind it.
///
/// # What it does not do
///
/// It does not close anything. It reports, through `on_dismiss`, and the caller decides — which is
/// what makes a confirmation dialog that refuses to close expressible without a second component.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::reactive::UnsyncCallback;
/// use zgui::{component, view};
/// use zgui_ui_primitives::prelude::*;
///
/// #[component]
/// fn Menu(open: RwSignal<bool, zgui::reactive::LocalStorage>) -> impl IntoView {
///     view! {
///         Show(when = move || open.get(), fallback = || view! { box() }) {
///             DismissableLayer(
///                 layer = OverlayLayer::Popover,
///                 on_dismiss = UnsyncCallback::new(move |_reason: DismissReason| open.set(false))
///             ) {
///                 box(class = "menu") {"items"}
///             }
///         }
///     }
/// }
/// ```
#[component]
// A component's props *are* its arguments: a caller writes them by name, and counting them as a
// positional list measures something nobody who calls this ever sees.
#[allow(clippy::too_many_arguments)]
pub fn DismissableLayer(
    /// Told why the layer should close. It closes nothing itself.
    on_dismiss: UnsyncCallback<DismissReason>,
    /// Which overlay band this layer belongs to, which is what decides what is above what.
    #[prop(default = OverlayLayer::default())]
    layer: OverlayLayer,
    /// Whether a press outside the layer dismisses it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Whether Escape dismisses it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// One element outside the layer that a press on nonetheless belongs to it.
    ///
    /// What a trigger is. A surface anchored to a button sits nowhere near that button in the
    /// tree, so a press on the button is a press outside the surface — the layer dismisses, the
    /// press goes on to become a click, and the trigger opens the surface it has just closed. The
    /// surface then never closes from its own trigger, which is the one place every user tries
    /// first.
    #[prop(optional)]
    exclude: Option<NodeRef>,
    /// Where to record the layer's own element.
    #[prop(optional)]
    element_ref: Option<NodeRef>,
    /// Extra classes on the layer's own element.
    #[prop(into, optional)]
    class: Classes,
    /// What is inside the layer.
    children: Children,
) -> impl IntoView {
    let root = element_ref.unwrap_or_default();
    let stack = LayerStack::current();
    let id = stack.push(layer, root);
    note!(
        "layer.push",
        "id={} band={} open={} stack={}",
        id.get(),
        layer.name(),
        stack.len(),
        stack.describe()
    );
    {
        let stack = stack.clone();
        on_cleanup_local(move || {
            stack.pop(id);
            note!(
                "layer.pop",
                "id={} open={} stack={}",
                id.get(),
                stack.len(),
                stack.describe()
            );
        });
    }

    // What the stack is told about this layer's own life. A surface that has been asked to close
    // stays mounted for the length of its exit animation, and for that whole tenth of a second it
    // is still the innermost entry on the stack — so an Escape pressed in it is claimed by a
    // surface that is already going, and the dialog behind it never hears one. The enclosing
    // presence is what knows, and it publishes the same state the style sheet animates on.
    let leaving = {
        let stack = stack.clone();
        let presence = crate::presence::use_presence();
        RenderEffect::new(move |was: Option<bool>| {
            let leaving = presence.is_some_and(|presence| presence.state().is_leaving());
            if was != Some(leaving) {
                note!("layer.leaving", "id={} leaving={leaving}", id.get());
            }
            stack.set_leaving(id, leaving);
            leaving
        })
    };

    // The listeners go on the *window's* root, because that is the only place a press somewhere
    // else in the window can be heard. They are guards rather than bindings for the same reason:
    // the root outlives this component, so nothing else would ever take them off.
    let guards: Rc<RefCell<Vec<ListenerGuard>>> = Rc::new(RefCell::new(Vec::new()));
    let attached = {
        let guards = Rc::clone(&guards);
        let stack = stack.clone();
        RenderEffect::new(move |_| {
            if root.get().is_none() || !guards.borrow().is_empty() {
                return;
            }
            let Some(window) = root.window_root() else {
                return;
            };

            let mut held = Vec::new();
            // The capture leg, so the decision is taken before anything inside the press's own
            // path runs. A layer that dismissed on the bubble would be dismissed by its own
            // trigger's click, which is the "the menu closes the instant it opens" fault.
            let dismiss = on_dismiss;
            let outside_stack = stack.clone();
            held.extend(window.listen(
                events::POINTER_DOWN,
                ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::PointerDown>| {
                    if !dismiss_on_outside_press.get_untracked()
                        || !outside_stack.is_topmost(id)
                        || root.contains(ev.target)
                        || exclude.is_some_and(|anchor| anchor.contains(ev.target))
                    {
                        return;
                    }
                    note!(
                        "layer.dismiss",
                        "id={} reason=outside target={:?}",
                        id.get(),
                        ev.target
                    );
                    dismiss.run(DismissReason::OutsidePress);
                },
            ));

            let dismiss = on_dismiss;
            let escape_stack = stack.clone();
            held.extend(window.listen(
                events::KEY_DOWN,
                ListenerOptions::CAPTURE,
                move |ev: &mut EventCx<'_, events::KeyDown>| {
                    if ev.key == Key::Named(NamedKey::Escape) {
                        note!(
                            "layer.escape",
                            "id={} enabled={} answering={:?} mine={} stack={}",
                            id.get(),
                            dismiss_on_escape.get_untracked(),
                            escape_stack.answering_escape().map(LayerId::get),
                            escape_stack.answers_escape(id),
                            escape_stack.describe()
                        );
                    }
                    // The innermost layer that is still open, rather than the innermost one there
                    // is: a surface playing its exit animation is on the stack and is not open,
                    // and an Escape it claimed would be an Escape the dialog behind it never gets.
                    if !dismiss_on_escape.get_untracked()
                        || ev.key != Key::Named(NamedKey::Escape)
                        || !escape_stack.answers_escape(id)
                    {
                        return;
                    }
                    // Escape belongs to exactly one surface, and swallowing it here is what stops
                    // one press closing a menu and the dialog behind it in the same breath.
                    ev.stop_propagation();
                    ev.prevent_default();
                    note!("layer.dismiss", "id={} reason=escape", id.get());
                    dismiss.run(DismissReason::EscapeKey);
                },
            ));

            *guards.borrow_mut() = held;
        })
    };

    on_cleanup_local(move || {
        drop(leaving);
        drop(attached);
        guards.borrow_mut().clear();
    });

    view! {
        box(class = class, node_ref = root, attr:data-layer = layer.name()) {
            {children.into_view_once()}
        }
    }
}
