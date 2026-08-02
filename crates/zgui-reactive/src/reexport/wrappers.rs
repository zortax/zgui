//! The types a component property is declared as.
//!
//! A property that says `Signal<T>` accepts a constant, a signal, a memo or a closure, because
//! all four convert into one. That is what lets a caller write `count=5`, `count=my_signal` or
//! `count=move || a.get() + 1` against a single declaration.

/// Any reactive source of a `T`, whatever it was built from.
///
/// The type to declare a component property as. Reading it subscribes to whatever is behind it;
/// a constant simply never changes.
pub use reactive_graph::wrappers::read::Signal;

/// The reference-counted form of [`Signal`].
pub use reactive_graph::wrappers::read::ArcSignal;

/// An optional reactive property: absent, a constant, or a signal.
///
/// The declaration for a property with a meaningful "not set" state, as distinct from one set
/// to a default.
pub use reactive_graph::wrappers::read::MaybeProp;

/// A reactive sink for a `T`: something a component can write back through.
///
/// The counterpart of [`Signal`] for two-way properties, satisfied by a signal, a write half or
/// a closure.
pub use reactive_graph::wrappers::write::SignalSetter;
