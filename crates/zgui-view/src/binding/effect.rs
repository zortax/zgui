//! One reactive binding from a computed value to one property of one element.

use core::any::Any;

use zgui_reactive::RenderEffect;

use crate::cx::BuildCx;
use crate::value::ReactiveValue;

/// A reactive binding from a computed value to one property of one element.
///
/// The first computation runs immediately, on construction. Later runs happen during the frame's
/// reactive flush, and **a run whose value equals the previous one does no backend work at all** —
/// signals have no equality gate of their own, so this comparison is what turns "a signal was
/// written" into "a value actually changed".
///
/// A binding over a constant creates no effect: the value is written once and there is nothing to
/// re-run.
///
/// Rule of thumb: reach for a memo when a value is shared by two or more bindings or is expensive
/// to compute, and rely on this comparison when it is used once.
#[must_use = "a binding stops updating when it is dropped"]
pub struct Binding(Option<Box<dyn Any>>);

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
                let effect = cx.with_owner(|| {
                    RenderEffect::new(move |previous: Option<T>| {
                        let next = compute();
                        match previous {
                            Some(previous) if previous == next => next,
                            _ => {
                                write(&next);
                                next
                            }
                        }
                    })
                });
                // Boxed as `Any` only to keep the effect alive without the binding's own type
                // naming the value type it happens to compare against.
                Self(Some(Box::new(effect)))
            }
        }
    }

    /// Whether this binding has an effect behind it.
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
    use zgui_reactive::{RwSignal, flush};

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
}
