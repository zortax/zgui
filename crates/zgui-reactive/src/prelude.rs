//! The traits that make signals readable and writable.
//!
//! Reading and writing are trait methods, not inherent ones, so that a component can accept
//! "anything readable as a `T`" without caring whether it was handed a signal, a memo, a store
//! field or a constant. Those traits have to be in scope for `get`, `set`, `update`, `with` and
//! `track` to resolve, which is what this module is for.
//!
//! ```
//! use zgui_reactive::prelude::*;
//! use zgui_reactive::{Mounted, RwSignal, install};
//!
//! install().unwrap();
//! let node = Mounted::new();
//! let count = node.with(|| RwSignal::new(0));
//!
//! count.set(1);
//! count.update(|n| *n += 1);
//! assert_eq!(count.get(), 2);
//! node.unmount();
//! ```
//!
//! Every trait has an `_untracked` counterpart that reads without subscribing. Reach for it only
//! where not subscribing is the point; a read that should have been tracked and was not
//! produces a view that renders once and then never changes, with nothing to see in a debugger.

pub use reactive_graph::prelude::*;
