//! Whether one surface is open, what opened it, and what it opened.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::AttrName;
use zgui::vocab::HasPopup;
use zgui_ui_primitives::{Binding, Controllable};

/// One overlay's open state, and the two elements it is written between.
///
/// Every floating surface here — a dialog, a menu, a tooltip, a select's list — is at least three
/// components apart: a root that owns whether it is open, a trigger somewhere inside it, and a
/// surface that is portalled out of the tree altogether. They cannot pass anything to each other
/// directly, and each of them needs all three of the same facts: *is it open*, *what is it
/// anchored to*, and *what is being anchored*. So the root publishes this and the other two find
/// it.
///
/// `Copy`, so a handler stores one without cloning, and reachable from any depth with
/// [`OverlayState::current`].
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::overlay::OverlayState;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let state = OverlayState::uncontrolled(false, None).provide();
///     assert!(!state.is_open());
///     assert_eq!(state.state_name(), "closed");
///
///     state.toggle();
///     assert!(state.is_open());
///     assert_eq!(state.state_name(), "open");
///
///     // And anything below the root finds the same one.
///     assert!(OverlayState::current().is_some_and(|found| found.is_open()));
/// });
/// scope.unmount();
/// ```
#[derive(Copy, Clone)]
pub struct OverlayState {
    /// Whether the surface is open, owned by whoever asked to own it.
    open: Controllable<bool>,
    /// The element the surface is anchored to and returns focus to.
    trigger: NodeRef,
    /// The surface itself, once it has been built.
    content: NodeRef,
}

impl OverlayState {
    /// Wires up an overlay from a component's three open-state props.
    ///
    /// A writable `open` is the surface's own state, opened and closed by pressing the trigger. A
    /// [`Binding::controlled`] one leaves the decision with the caller, and the surface does not
    /// move until the caller moves it — which is what makes "confirm before closing" expressible
    /// without a second component.
    #[must_use]
    pub fn new(
        open: Binding<bool>,
        default_open: bool,
        on_open_change: Option<UnsyncCallback<bool>>,
    ) -> Self {
        Self {
            open: Controllable::new(open, default_open, on_open_change),
            trigger: NodeRef::new(),
            content: NodeRef::new(),
        }
    }

    /// The same, for an overlay nothing outside it will ever control.
    #[must_use]
    pub fn uncontrolled(default_open: bool, on_open_change: Option<UnsyncCallback<bool>>) -> Self {
        Self::new(Binding::Unbound, default_open, on_open_change)
    }

    /// Publishes this to every scope below the current one, and hands it back.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The overlay the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether the surface is open, subscribing to it.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn is_open_untracked(&self) -> bool {
        self.open.get_untracked()
    }

    /// Whether the surface is open, as a signal to bind.
    #[must_use]
    pub fn open_signal(&self) -> Signal<bool, LocalStorage> {
        self.open.signal()
    }

    /// Opens or closes the surface, and tells whoever asked to be told.
    pub fn set_open(&self, open: bool) {
        self.open.set(open);
    }

    /// Opens it.
    pub fn open(&self) {
        self.set_open(true);
    }

    /// Closes it.
    pub fn close(&self) {
        self.set_open(false);
    }

    /// Opens it if it is closed, and closes it if it is open.
    pub fn toggle(&self) {
        self.open.toggle();
    }

    /// The element the surface is anchored to, and that focus goes back to.
    #[must_use]
    pub fn trigger(&self) -> NodeRef {
        self.trigger
    }

    /// The surface itself.
    #[must_use]
    pub fn content(&self) -> NodeRef {
        self.content
    }

    /// How the open state is written as an attribute value.
    ///
    /// The one thing a style sheet selects on to animate a surface in and out, and the reason no
    /// component here has an `is_open` class: `[data-state="open"]` reads as the state it is,
    /// and it is the same word on every component in this library.
    #[must_use]
    pub fn state_name(&self) -> &'static str {
        if self.is_open() { "open" } else { "closed" }
    }

    /// What a trigger puts on its own element.
    ///
    /// Three things a reader needs and one a style sheet does: that the control opens a surface of
    /// this kind, whether that surface is showing, which element it is, and the open state as an
    /// attribute so a chevron can be turned upside down in CSS.
    #[must_use]
    pub fn trigger_attrs(&self, popup: HasPopup) -> Attrs {
        let state = *self;
        Attrs::new()
            .attribute(AttrName::new("data-state"), move || {
                Some(state.state_name().to_owned())
            })
            .a11y_from(
                A11yBinding::unspecified()
                    .has_popup(popup)
                    .expanded(move || state.is_open())
                    .controls(self.content),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui::prelude::*;
    use zgui::reactive::{LocalStorage, Mounted, RwSignal, UnsyncCallback, install};

    use super::{Binding, OverlayState};

    #[test]
    fn a_controlling_caller_is_told_and_the_surface_waits_for_it() {
        // A confirmation dialog is exactly this: the close is reported, the caller decides, and
        // until it does the dialog is still there.
        install().ok();
        let scope = Mounted::new();
        let (state, held, seen) = scope.with(|| {
            let held: RwSignal<bool, LocalStorage> = RwSignal::new_local(true);
            let seen = Rc::new(RefCell::new(Vec::new()));
            let record = Rc::clone(&seen);
            // A caller that reports every close and refuses all of them until it decides.
            let state = OverlayState::new(
                Binding::controlled(held, |_: bool| {}),
                false,
                Some(UnsyncCallback::new(move |open: bool| {
                    record.borrow_mut().push(open);
                })),
            );
            (state, held, seen)
        });

        state.close();
        assert_eq!(*seen.borrow(), [false], "the caller was told");
        assert!(state.is_open_untracked(), "and nothing closed on its own");

        held.set(false);
        assert!(!state.is_open_untracked());
        scope.unmount();
    }

    #[test]
    fn the_state_name_is_the_word_a_style_sheet_selects_on() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let state = OverlayState::uncontrolled(false, None);
            assert_eq!(state.state_name(), "closed");
            state.open();
            assert_eq!(state.state_name(), "open");
        });
        scope.unmount();
    }
}
