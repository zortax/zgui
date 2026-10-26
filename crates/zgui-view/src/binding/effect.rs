//! One reactive binding from a computed value to one property of one element.

use zgui_reactive::Sink;

use crate::cx::BuildCx;
use crate::value::ReactiveValue;

/// A reactive binding from a computed value to one property of one element.
///
/// The first computation runs immediately, on construction. Later runs happen during the frame's
/// reactive flush, and **a run whose value equals the previous one does no backend work at all** —
/// signals have no equality gate of their own, so this comparison is what turns "a signal was
/// written" into "a value actually changed".
///
/// A binding over a constant creates no reactive node: the value is written once and there is
/// nothing to re-run. A dynamic one is a [`Sink`], which has no owner and no task of its own.
/// Its closure runs under the view's owner with no cleanup between runs, so a memo, signal,
/// context or cleanup that it creates is kept until the view is unmounted. Create those in the
/// component body, or use a reactive hole for a closure that builds them.
///
/// Rule of thumb: reach for a memo when a value is shared by two or more bindings or is expensive
/// to compute, and rely on this comparison when it is used once.
#[must_use = "a binding stops updating when it is dropped"]
pub struct Binding(Option<Sink>);

impl Binding {
    /// Writes `value` through `write`, and keeps writing it whenever it changes.
    pub fn new<T>(
        cx: &BuildCx<'_>,
        value: ReactiveValue<T>,
        mut write: impl FnMut(&T) + 'static,
    ) -> Self
    where
        T: Clone + PartialEq + 'static,
    {
        match value {
            ReactiveValue::Constant(value) => {
                write(&value);
                Self(None)
            }
            ReactiveValue::Dynamic(compute) => {
                let mut previous: Option<T> = None;
                let sink = cx.with_owner(|| {
                    Sink::new(move || {
                        let next = compute();
                        if previous.as_ref() != Some(&next) {
                            write(&next);
                        }
                        previous = Some(next);
                    })
                });
                Self(Some(sink))
            }
        }
    }

    /// Whether this binding has a reactive node behind it.
    ///
    /// `false` for a binding over a constant, which is the whole point of deciding static against
    /// dynamic by type: a literal attribute costs one write and no reactive node.
    pub fn is_reactive(&self) -> bool {
        self.0.is_some()
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, Sink, flush};

    use super::Binding;
    use crate::fixture::Fixture;
    use crate::value::{IntoReactiveValue, ReactiveValue};

    #[test]
    fn a_constant_binding_writes_once_and_creates_no_effect() {
        let f = Fixture::new();
        let writes = Rc::new(Cell::new(0));
        let counter = Rc::clone(&writes);

        let binding = f.window.with(|| {
            Binding::new(&f.cx(), ReactiveValue::Constant(1i32), move |_| {
                counter.set(counter.get() + 1);
            })
        });

        assert_eq!(writes.get(), 1);
        assert!(!binding.is_reactive());
        f.window.unmount();
    }

    #[test]
    fn a_run_whose_value_did_not_change_writes_nothing() {
        let f = Fixture::new();
        let writes = Rc::new(Cell::new(0));
        let counter = Rc::clone(&writes);
        let source = f.window.with(|| RwSignal::new(2i32));

        let _binding = f.window.with(|| {
            let value = (move || source.get() % 2).into_reactive_value();
            Binding::new(&f.cx(), value, move |_: &i32| {
                counter.set(counter.get() + 1)
            })
        });
        assert_eq!(writes.get(), 1, "the first computation runs immediately");

        source.set(4); // a different signal value, the same computed one
        flush();
        assert_eq!(writes.get(), 1);

        source.set(5);
        flush();
        assert_eq!(writes.get(), 2);
        f.window.unmount();
    }

    #[test]
    fn dropping_a_binding_frees_its_sink() {
        let f = Fixture::new();
        let before = Sink::live();
        let source = f.window.with(|| RwSignal::new(0i32));
        let writes = Rc::new(Cell::new(0));
        let counter = Rc::clone(&writes);

        let binding = f.window.with(|| {
            Binding::new(
                &f.cx(),
                (move || source.get()).into_reactive_value(),
                move |_: &i32| counter.set(counter.get() + 1),
            )
        });
        assert_eq!(Sink::live(), before + 1);

        source.set(1);
        drop(binding);
        assert_eq!(Sink::live(), before);
        flush();
        assert_eq!(writes.get(), 1, "a dropped binding writes nothing");
        f.window.unmount();
    }

    #[test]
    fn a_binding_is_eight_bytes() {
        // The sink's key: a slot index and a non-zero generation, which is the niche.
        assert_eq!(size_of::<Binding>(), 8);
    }
}
