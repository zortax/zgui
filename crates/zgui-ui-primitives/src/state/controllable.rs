//! A value that works the same whether the caller owns it or the component does.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};

use crate::state::binding::Binding;

/// A value that works the same whether the caller owns it or the component does.
///
/// Every component with a value has to answer the same question twice over: *who owns it?* A
/// checkbox used on its own owns its own checked state; the same checkbox inside a form owns
/// nothing and reflects what the form says. Writing the two separately is how a library ends up
/// with a `Checkbox` and a `ControlledCheckbox` that drift apart.
///
/// So there is one: a component takes a [`Binding`], a `default_value` and an `on_change`, hands
/// all three to [`Controllable::new`], and from then on reads and writes it as if it owned it.
///
/// | Binding | What a write does |
/// |---|---|
/// | [`Binding::Unbound`] | moves the value the component keeps |
/// | [`Binding::TwoWay`] | sets the caller's signal, which is what moves the value |
/// | [`Binding::Controlled`] | asks the caller, and the value moves when the caller moves it |
///
/// `on_change` is told about the change in all three, after the binding has been asked. It is an
/// observer — somewhere to log, to mark a form dirty, to close a menu — and not the thing that
/// makes a bound control work, which is the binding.
///
/// A write of the value it already holds does nothing and fires nothing, because a callback that
/// fires when nothing changed is a re-render loop waiting for a caller that echoes it back.
///
/// ```
/// use zgui::reactive::{Mounted, RwSignal, install};
/// use zgui::prelude::*;
/// use zgui_ui_primitives::{Binding, Controllable};
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     // Uncontrolled: the component owns it.
///     let open = Controllable::uncontrolled(false, None);
///     assert!(!open.get_untracked());
///     open.set(true);
///     assert!(open.get_untracked());
///
///     // Two-way: the caller's signal is the value, and a press moves it.
///     let held = RwSignal::new_local(false);
///     let open = Controllable::new(held.into(), false, None);
///     open.set(true);
///     assert!(held.get_untracked(), "the press reached the caller's signal");
///     held.set(false);
///     assert!(!open.get_untracked(), "and the caller can move it back");
/// });
/// scope.unmount();
/// ```
pub struct Controllable<T: Clone + PartialEq + 'static> {
    /// What the caller has tied the value to, when the caller has tied it to anything.
    binding: Binding<T>,
    /// What the component says it is, used only when nothing is bound.
    internal: RwSignal<T, LocalStorage>,
    /// Told about every change, whoever owns the value.
    on_change: Option<UnsyncCallback<T>>,
}

impl<T: Clone + PartialEq + 'static> Controllable<T> {
    /// Wires up a value from a component's three props.
    ///
    /// `default_value` is what the value starts at when `binding` is
    /// [`Unbound`](Binding::Unbound), and is unused otherwise: a caller who has bound a value has
    /// already said what it starts at.
    pub fn new(
        binding: Binding<T>,
        default_value: T,
        on_change: Option<UnsyncCallback<T>>,
    ) -> Self {
        Self {
            binding,
            internal: RwSignal::new_local(default_value),
            on_change,
        }
    }

    /// The same, for a component that is only ever uncontrolled.
    pub fn uncontrolled(default_value: T, on_change: Option<UnsyncCallback<T>>) -> Self {
        Self::new(Binding::Unbound, default_value, on_change)
    }

    /// What the caller tied this to.
    #[must_use]
    pub fn binding(&self) -> Binding<T> {
        self.binding
    }

    /// The value now, subscribing to it.
    pub fn get(&self) -> T {
        match self.binding.get() {
            Some(value) => value,
            None => self.internal.get(),
        }
    }

    /// The value now, without subscribing.
    pub fn get_untracked(&self) -> T {
        match self.binding.get_untracked() {
            Some(value) => value,
            None => self.internal.get_untracked(),
        }
    }

    /// The value as a signal, for handing to a binding.
    pub fn signal(&self) -> Signal<T, LocalStorage> {
        let controllable = *self;
        Signal::derive_local(move || controllable.get())
    }

    /// Whether anybody outside the component is holding the value.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.binding.is_bound()
    }

    /// Whether a write goes to a caller's callback rather than to something this can set.
    ///
    /// True only of [`Binding::Controlled`]: a two-way binding is a signal this writes, so the
    /// value moves as soon as it is asked to, exactly as an unbound one does.
    #[must_use]
    pub fn is_controlled(&self) -> bool {
        self.binding.is_controlled()
    }

    /// Moves the value to `next`, and tells whoever asked to be told.
    ///
    /// Under a [`Binding::Controlled`] this reports and waits: the value does not move until the
    /// caller moves it, which is what makes "the caller refused the change" expressible at all.
    pub fn set(&self, next: T) {
        if self.get_untracked() == next {
            return;
        }
        match self.binding {
            Binding::Unbound => self.internal.set(next.clone()),
            _ => self.binding.write(next.clone()),
        }
        if let Some(on_change) = &self.on_change {
            on_change.run(next);
        }
    }

    /// Reads the value, changes it, and writes it back.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        let mut next = self.get_untracked();
        change(&mut next);
        self.set(next);
    }
}

impl<T: Clone + PartialEq + 'static> Clone for Controllable<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + PartialEq + 'static> Copy for Controllable<T> {}

impl Controllable<bool> {
    /// Flips a boolean value.
    pub fn toggle(&self) {
        self.set(!self.get_untracked());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui::prelude::*;
    use zgui::reactive::{LocalStorage, Mounted, RwSignal, UnsyncCallback, flush, install};

    use super::{Binding, Controllable};

    /// A callback that writes down everything it was told.
    fn recorder() -> (UnsyncCallback<bool>, Rc<RefCell<Vec<bool>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let record = Rc::clone(&seen);
        (
            UnsyncCallback::new(move |value: bool| record.borrow_mut().push(value)),
            seen,
        )
    }

    #[test]
    fn an_uncontrolled_value_moves_itself_and_reports_the_move() {
        install().ok();
        let scope = Mounted::new();
        let seen = scope.with(|| {
            let (on_change, seen) = recorder();
            let open = Controllable::uncontrolled(false, Some(on_change));

            open.set(true);
            assert!(open.get_untracked());
            open.toggle();
            assert!(!open.get_untracked());
            open.update(|open| *open = true);
            assert!(open.get_untracked());
            seen
        });
        assert_eq!(*seen.borrow(), [true, false, true]);
        scope.unmount();
    }

    #[test]
    fn a_writable_signal_bound_with_no_callback_at_all_still_moves() {
        // The shape an application actually writes — `checked=some_signal`, nothing else — and the
        // one that used to leave a control that could not be operated at all.
        install().ok();
        let scope = Mounted::new();
        let (held, open) = scope.with(|| {
            let held: RwSignal<bool, LocalStorage> = RwSignal::new_local(false);
            (held, Controllable::new(held.into(), false, None))
        });

        open.toggle();
        assert!(held.get_untracked(), "the caller's signal moved");
        assert!(open.get_untracked(), "and so did what the control shows");
        open.toggle();
        assert!(!held.get_untracked());
        scope.unmount();
    }

    #[test]
    fn a_two_way_binding_tells_an_observer_as_well_as_writing_the_signal() {
        install().ok();
        let scope = Mounted::new();
        let (held, open, seen) = scope.with(|| {
            let held: RwSignal<bool, LocalStorage> = RwSignal::new_local(false);
            let (on_change, seen) = recorder();
            (
                held,
                Controllable::new(held.into(), false, Some(on_change)),
                seen,
            )
        });

        open.set(true);
        assert!(held.get_untracked());
        assert_eq!(*seen.borrow(), [true]);
        scope.unmount();
    }

    #[test]
    fn a_controlled_value_reports_the_change_and_waits_for_the_caller_to_make_it() {
        install().ok();
        let scope = Mounted::new();
        let (open, seen, held) = scope.with(|| {
            let held: RwSignal<bool, LocalStorage> = RwSignal::new_local(false);
            let (on_change, seen) = recorder();
            // A caller that refuses every change: the value is read from `held` and writes go
            // nowhere near it.
            let binding = Binding::controlled(held, |_: bool| {});
            let open = Controllable::new(binding, false, Some(on_change));
            (open, seen, held)
        });

        open.set(true);
        assert_eq!(*seen.borrow(), [true], "the caller was told");
        assert!(
            !open.get_untracked(),
            "and the value did not move on its own"
        );

        // The caller accepts it.
        held.set(true);
        assert!(open.get_untracked());
        scope.unmount();
    }

    #[test]
    fn a_controlled_binding_that_accepts_the_change_moves_at_once() {
        install().ok();
        let scope = Mounted::new();
        let (open, held) = scope.with(|| {
            let held: RwSignal<u8, LocalStorage> = RwSignal::new_local(0);
            let binding = Binding::controlled(
                Signal::derive_local(move || held.get() > 0),
                move |on: bool| held.set(u8::from(on)),
            );
            (Controllable::new(binding, false, None), held)
        });

        open.set(true);
        assert_eq!(held.get_untracked(), 1);
        assert!(open.get_untracked());
        scope.unmount();
    }

    #[test]
    fn writing_the_value_it_already_holds_tells_nobody() {
        // A callback that fires when nothing changed is a loop with any caller that echoes it
        // back, and that loop is invisible in every assertion about the value itself.
        install().ok();
        let scope = Mounted::new();
        let seen = scope.with(|| {
            let (on_change, seen) = recorder();
            let open = Controllable::uncontrolled(false, Some(on_change));
            open.set(false);
            open.set(false);
            seen
        });
        assert!(seen.borrow().is_empty());
        scope.unmount();
    }

    #[test]
    fn a_reader_of_the_signal_follows_whichever_owner_is_in_force() {
        install().ok();
        let scope = Mounted::new();
        let (reads, held, open) = scope.with(|| {
            let held: RwSignal<bool, LocalStorage> = RwSignal::new_local(false);
            let open = Controllable::new(held.into(), false, None);
            let value = open.signal();
            let reads = Rc::new(RefCell::new(Vec::new()));
            let record = Rc::clone(&reads);
            let effect = zgui::reactive::RenderEffect::new(move |_| {
                record.borrow_mut().push(value.get());
            });
            core::mem::forget(effect);
            (reads, held, open)
        });

        // The control writes, and the reader sees it because the caller's signal moved.
        open.set(true);
        flush();
        // The caller writes, and the reader sees that too.
        held.set(false);
        flush();

        assert_eq!(*reads.borrow(), [false, true, false]);
        scope.unmount();
    }
}
