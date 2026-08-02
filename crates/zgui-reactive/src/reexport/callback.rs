//! Callback properties.

/// A callback property that can cross threads.
///
/// `Copy` and freed with the owner that created it, so a component can store one without
/// generics and without boxing at every call site.
pub use reactive_graph::callback::Callback;

/// What makes a callback callable.
///
/// `run` is a trait method rather than an inherent one, so a component can accept "anything
/// callable with a `T`" without naming which callback type it was handed. The trait has to be in
/// scope for `run` to resolve, which is why it is published beside the two callbacks and carried
/// in the prelude — without it, a component holding a callback prop has no way to call it.
pub use reactive_graph::callback::Callable;

/// A callback property that cannot cross threads.
///
/// The variant to use for anything closing over the view: node handles, element references and
/// document state are all deliberately thread-local.
pub use reactive_graph::callback::UnsyncCallback;
