//! What a caller has tied a component's value to.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};

/// A caller's hold on a component's value.
///
/// A control with a value has to answer *who owns it*, and the answer decides what a click does.
/// The three answers are the three variants here, and which one a caller gets is decided by the
/// type of what they pass — not by a second prop that could disagree with the first.
///
/// | What the caller writes | Which variant | What a click does |
/// |---|---|---|
/// | nothing | [`Unbound`](Binding::Unbound) | moves the component's own value |
/// | `checked=signal`, an [`RwSignal`] | [`TwoWay`](Binding::TwoWay) | writes the caller's signal, which moves the control |
/// | `checked=Binding::controlled(read, write)` | [`Controlled`](Binding::Controlled) | calls `write`, and the control moves only if `write` moves `read` |
///
/// # Why a writable signal binds both ways
///
/// A signal carries its write capability in its type: an [`RwSignal`] can be set and a
/// [`Signal`] cannot. So a caller who hands a control an `RwSignal` has handed it the ability to
/// write, and a control that only *read* it would be refusing a capability it was given — which
/// looks, on screen, exactly like a control that is broken. Binding an `RwSignal` therefore binds
/// both ways.
///
/// # Why a read-only signal cannot be bound on its own
///
/// There is no conversion from [`Signal`] into this type, so a read-only signal is a compile
/// error at the prop rather than a control that never moves. The way to drive a control from a
/// value you compute is [`Binding::controlled`], which takes the write side as an argument — so
/// "the caller controls it and nothing can change it" is not a thing this type can express by
/// accident. A control that genuinely must not move is a `disabled` one, and says so.
///
/// ```compile_fail
/// use zgui::prelude::*;
/// use zgui::reactive::{LocalStorage, RwSignal};
/// use zgui_ui_primitives::Binding;
///
/// fn frozen(read: Signal<bool, LocalStorage>) -> Binding<bool> {
///     // No `From<Signal<…>>`, so this does not compile — which is where a control that
///     // reflects a value and can never be operated used to come from.
///     read.into()
/// }
/// ```
///
/// ```
/// use zgui::reactive::{Mounted, RwSignal, install};
/// use zgui::prelude::*;
/// use zgui_ui_primitives::Binding;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     // A writable signal binds both ways.
///     let ticked = RwSignal::new_local(false);
///     let binding = Binding::from(ticked);
///     binding.write(true);
///     assert!(ticked.get_untracked(), "the control wrote back to the caller's signal");
///
///     // A computed value needs the write side spelling out.
///     let held = RwSignal::new_local(0_u8);
///     let binding = Binding::controlled(
///         Signal::derive_local(move || held.get() > 0),
///         move |on: bool| held.set(u8::from(on)),
///     );
///     binding.write(true);
///     assert_eq!(held.get_untracked(), 1);
/// });
/// scope.unmount();
/// ```
#[derive(Default)]
pub enum Binding<T: Clone + PartialEq + 'static> {
    /// Nobody outside is holding it, so the component owns it.
    #[default]
    Unbound,
    /// The caller's own writable signal, read from and written back to.
    TwoWay(RwSignal<T, LocalStorage>),
    /// The caller's value, and what to call instead of writing it.
    Controlled {
        /// What the value is.
        read: Signal<T, LocalStorage>,
        /// Told what the control would like it to become.
        write: UnsyncCallback<T>,
    },
}

impl<T: Clone + PartialEq + 'static> Binding<T> {
    /// Binds `read` for display and `write` for every change the control asks for.
    ///
    /// This is the form for a value the caller keeps somewhere a signal cannot be written
    /// directly — a field of a struct held in one signal, a value that has to be validated before
    /// it moves, a value that lives on the far side of a message queue. `write` may decline: the
    /// control shows whatever `read` says and nothing else, so a `write` that ignores its argument
    /// is a control the caller has refused to let move.
    pub fn controlled(
        read: impl Into<Signal<T, LocalStorage>>,
        write: impl Fn(T) + 'static,
    ) -> Self {
        Self::Controlled {
            read: read.into(),
            write: UnsyncCallback::new(write),
        }
    }

    /// Whether anybody outside the component is holding the value.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        !matches!(self, Self::Unbound)
    }

    /// Whether changes go to a caller's callback rather than to a signal this can write.
    #[must_use]
    pub fn is_controlled(&self) -> bool {
        matches!(self, Self::Controlled { .. })
    }

    /// What the caller says the value is, subscribing to it, or `None` when nothing is bound.
    #[must_use]
    pub fn get(&self) -> Option<T> {
        match self {
            Self::Unbound => None,
            Self::TwoWay(signal) => Some(signal.get()),
            Self::Controlled { read, .. } => Some(read.get()),
        }
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn get_untracked(&self) -> Option<T> {
        match self {
            Self::Unbound => None,
            Self::TwoWay(signal) => Some(signal.get_untracked()),
            Self::Controlled { read, .. } => Some(read.get_untracked()),
        }
    }

    /// Asks the caller for `next`, whichever way it is bound, and does nothing when it is not.
    pub fn write(&self, next: T) {
        match self {
            Self::Unbound => {}
            Self::TwoWay(signal) => signal.set(next),
            Self::Controlled { write, .. } => write.run(next),
        }
    }
}

impl<T: Clone + PartialEq + 'static> From<RwSignal<T, LocalStorage>> for Binding<T> {
    fn from(signal: RwSignal<T, LocalStorage>) -> Self {
        Self::TwoWay(signal)
    }
}

impl<T: Clone + PartialEq + 'static> Clone for Binding<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + PartialEq + 'static> Copy for Binding<T> {}

impl<T: Clone + PartialEq + 'static> core::fmt::Debug for Binding<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Unbound => "Binding::Unbound",
            Self::TwoWay(_) => "Binding::TwoWay",
            Self::Controlled { .. } => "Binding::Controlled",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui::prelude::*;
    use zgui::reactive::{Mounted, RwSignal, install};

    use super::Binding;

    #[test]
    fn an_unbound_binding_holds_nothing_and_swallows_writes() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let binding = Binding::<bool>::default();
            assert!(!binding.is_bound());
            assert_eq!(binding.get_untracked(), None);
            // Writing one is what a control does before it looks at who owns the value, so it has
            // to be harmless rather than a panic waiting for the first uncontrolled click.
            binding.write(true);
        });
        scope.unmount();
    }

    #[test]
    fn a_writable_signal_binds_both_ways() {
        install().ok();
        let scope = Mounted::new();
        let held = scope.with(|| {
            let held = RwSignal::new_local(1_u8);
            let binding: Binding<u8> = held.into();
            assert!(binding.is_bound());
            assert!(!binding.is_controlled());
            assert_eq!(binding.get_untracked(), Some(1));
            binding.write(2);
            held
        });
        assert_eq!(held.get_untracked(), 2, "the write reached the caller");
        scope.unmount();
    }

    #[test]
    fn a_controlled_binding_sends_its_writes_where_the_caller_said() {
        install().ok();
        let scope = Mounted::new();
        let (seen, binding) = scope.with(|| {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let record = Rc::clone(&seen);
            let held = RwSignal::new_local(0_u8);
            let binding =
                Binding::controlled(Signal::derive_local(move || held.get()), move |next: u8| {
                    record.borrow_mut().push(next);
                });
            (seen, binding)
        });
        assert!(binding.is_controlled());
        binding.write(7);
        assert_eq!(*seen.borrow(), [7]);
        assert_eq!(
            binding.get_untracked(),
            Some(0),
            "the caller was asked and has not answered, so the value has not moved"
        );
        scope.unmount();
    }
}
