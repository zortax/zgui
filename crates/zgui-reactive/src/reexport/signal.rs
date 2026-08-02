//! Signals: the values a frame reads and an event handler writes.

/// A readable and writable signal, freed with the owner that created it.
///
/// `Copy`, so it can be captured by any number of closures without ceremony. Reading it inside
/// an effect or a view subscribes that effect to it; writing it marks every subscriber and asks
/// for a frame.
pub use reactive_graph::signal::RwSignal;

/// The reference-counted form of [`RwSignal`], freed when the last handle is dropped.
///
/// Not `Copy`. Use it for state whose lifetime is not a scope's — a cache shared between two
/// subtrees, or a value handed to something outside the view tree.
pub use reactive_graph::signal::ArcRwSignal;

/// The read half of a split signal.
pub use reactive_graph::signal::ReadSignal;

/// The write half of a split signal.
///
/// Handing a component only this half is how "you may set this, you may not read it" is
/// expressed.
pub use reactive_graph::signal::WriteSignal;

/// The reference-counted read half of a split signal.
pub use reactive_graph::signal::ArcReadSignal;

/// The reference-counted write half of a split signal.
pub use reactive_graph::signal::ArcWriteSignal;

/// A signal with no value: something to depend on and something to notify.
///
/// For invalidating a derived value whose input is not itself reactive — a file on disk, a
/// clock, a hand-maintained cache.
pub use reactive_graph::signal::Trigger;

/// The reference-counted form of [`Trigger`].
pub use reactive_graph::signal::ArcTrigger;

/// Creates a signal, returning its read and write halves separately.
pub use reactive_graph::signal::signal;

/// Creates a signal holding a value that cannot cross threads.
///
/// The escape hatch for state that is genuinely local: a node handle, an `Rc`, anything from
/// the view layer. Reading it from another thread panics rather than being undefined.
pub use reactive_graph::signal::signal_local;

/// Creates a reference-counted signal, returning its read and write halves separately.
pub use reactive_graph::signal::arc_signal;
