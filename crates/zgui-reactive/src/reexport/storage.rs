//! Where a signal keeps its value, and therefore which threads may read it.

/// The storage a signal holding a value that cannot cross threads uses.
///
/// Every handle type in this crate is generic over its storage, and the default is the
/// thread-safe one. Anything a view holds — a node handle, a reference-counted callback, a
/// backend handle — is not thread-safe, so a signal carrying one is written
/// `RwSignal<T, LocalStorage>` or `Signal<T, LocalStorage>`.
///
/// The choice is a promise checked at runtime rather than at compile time: reading a
/// local-storage signal from another thread panics with a message naming the thread, instead of
/// being undefined.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{LocalStorage, Mounted, RwSignal, install};
/// use std::rc::Rc;
///
/// install().unwrap();
/// let node = Mounted::new();
/// let handle = node.with(|| RwSignal::<Rc<str>, LocalStorage>::new_local("a".into()));
/// assert_eq!(&*handle.get(), "a");
/// node.unmount();
/// ```
pub use reactive_graph::owner::LocalStorage;

/// The storage a signal holding a value that can cross threads uses, which is the default.
pub use reactive_graph::owner::SyncStorage;

/// What a storage kind promises about the values it keeps.
///
/// Named here because a function generic over storage has to bound by it; most code names one of
/// the two concrete kinds instead.
pub use reactive_graph::owner::Storage;
