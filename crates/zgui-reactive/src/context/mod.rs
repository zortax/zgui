//! Values passed down a scope tree instead of through every intervening call.
//!
//! A context is keyed by its type and looked up by walking from the current scope towards the
//! root. Providing one in a scope makes it visible to everything mounted below, and shadows any
//! outer value of the same type for that subtree.
//!
//! Two rules keep contexts from failing silently. Providing with no current owner discards the
//! value — [`provide_context`] and [`provide_local_context`] assert against that in debug
//! builds. And a lookup that finds nothing returns `None` rather than a default, because a
//! wrong default is far harder to notice than a missing value; [`expect_context`] is the
//! shorthand for "this is a programming error".
//!
//! Use a **newtype** for anything whose type is not already specific — `String` or `bool` as a
//! context key collides with every other use of that type in the process.

mod local;

pub use local::{provide_local_context, use_local_context};
pub use reactive_graph::owner::{take_context, update_context, with_context};

use crate::executor::assert_owner;

/// Makes `value` available to every scope below the current one.
///
/// Requires the value to be shareable across threads; use [`provide_local_context`] for
/// anything from the view layer, which is deliberately not.
///
/// ```
/// use zgui_reactive::{Mounted, install, provide_context, use_context};
///
/// #[derive(Clone, Copy, PartialEq, Debug)]
/// struct Density(u8);
///
/// install().unwrap();
/// let root = Mounted::new();
/// root.with(|| {
///     provide_context(Density(2));
///     let child = Mounted::new();
///     assert_eq!(child.with(use_context::<Density>), Some(Density(2)));
///     child.unmount();
/// });
/// root.unmount();
/// ```
///
/// # Panics
///
/// In debug builds, if there is no current owner: the value would otherwise be dropped on the
/// spot and every lookup below would return nothing, with no error anywhere.
#[track_caller]
pub fn provide_context<T: Send + Sync + 'static>(value: T) {
    assert_owner("provide_context");
    reactive_graph::owner::provide_context(value);
}

/// Looks up the nearest context of type `T`, or `None` if no scope above provides one.
#[track_caller]
pub fn use_context<T: Clone + 'static>() -> Option<T> {
    reactive_graph::owner::use_context()
}

/// Looks up the nearest context of type `T`, panicking if no scope above provides one.
///
/// For a component that cannot work without its parent — a menu item outside a menu, a tab
/// outside a tab list — where a missing context is a mistake in the view, not a state to
/// handle.
///
/// # Panics
///
/// If no scope above the current one provides a `T`.
#[track_caller]
pub fn expect_context<T: Clone + 'static>() -> T {
    reactive_graph::owner::expect_context()
}
