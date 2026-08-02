//! Context for values that cannot cross threads.

use send_wrapper::SendWrapper;

use crate::executor::{assert_owner, assert_ui_thread};

/// A value parked in the context table under its own type.
///
/// Wrapping is what lets a value that is not `Send` sit in a table the engine requires to be
/// shareable; the wrapper refuses to be touched from any thread but the one that created it,
/// which is the invariant that makes that sound.
struct Local<T>(SendWrapper<T>);

/// Makes `value` available to every scope below the current one, without requiring it to be
/// `Send`.
///
/// The variant to use for anything from the view layer — node handles, element references,
/// callbacks that capture them — none of which can cross threads. Use
/// [`provide_context`](crate::provide_context) for plain data.
///
/// Contexts are keyed by type, so providing a second value of the same type in the same scope
/// replaces the first, and providing one in a nested scope shadows the outer one for that
/// subtree.
///
/// # Panics
///
/// In debug builds, if called off the UI thread, or if there is no current owner — with no
/// owner the value is dropped immediately and every lookup below would silently return nothing.
#[track_caller]
pub fn provide_local_context<T: 'static>(value: T) {
    assert_ui_thread("provide_local_context");
    assert_owner("provide_local_context");
    reactive_graph::owner::provide_context(Local(SendWrapper::new(value)));
}

/// Looks up the nearest `!Send` context of type `T`, or `None` if no scope above provides one.
///
/// # Panics
///
/// In debug builds, if called off the UI thread. In any build, if the value was provided on a
/// different thread than the one reading it.
#[track_caller]
pub fn use_local_context<T: Clone + 'static>() -> Option<T> {
    assert_ui_thread("use_local_context");
    reactive_graph::owner::use_context::<Local<T>>().map(|local| local.0.take())
}

impl<T: Clone> Clone for Local<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use reactive_graph::owner::Owner;

    use super::*;
    use crate::executor::install;

    #[test]
    fn a_local_context_reaches_scopes_below_it() {
        install().unwrap();
        let outer = Owner::new();
        let handle = Rc::new(7u8);

        let seen = outer.with(|| {
            provide_local_context(Rc::clone(&handle));
            let inner = Owner::new();
            inner.with(use_local_context::<Rc<u8>>)
        });

        assert_eq!(seen.as_deref(), Some(&7));
        outer.cleanup();
    }

    #[test]
    fn an_unprovided_local_context_is_absent_rather_than_wrong() {
        install().unwrap();
        let owner = Owner::new();
        assert!(owner.with(use_local_context::<Rc<u8>>).is_none());
        owner.cleanup();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "requires a current owner")]
    fn providing_with_no_owner_panics_instead_of_leaking() {
        install().unwrap();
        provide_local_context(Rc::new(1u8));
    }
}
