//! Marking code that reads signals without wanting to depend on them.

/// Suppresses the "read outside a reactive tracking context" diagnostic until it is dropped.
///
/// See [`enter_non_reactive_zone`].
#[derive(Debug)]
pub struct NonReactiveZone {
    /// Restores the previous state when it is dropped.
    _guard: reactive_graph::diagnostics::SpecialNonReactiveZoneGuard,
}

/// Marks the code that follows as deliberately non-reactive.
///
/// Reading a signal where nothing is subscribing is usually a bug — a view that reads a value
/// instead of a closure renders once and never updates — so debug builds report every such
/// read, with the location and a list of the usual causes. Some code reads signals *correctly*
/// with nothing subscribing, and would otherwise drown that diagnostic in false positives:
///
/// * event handler bodies, which must run when the event fires and not when a value they happen
///   to read changes;
/// * timer and animation callbacks, for the same reason;
/// * assertions and diagnostics that inspect state without wanting to depend on it.
///
/// The frame loop enters a zone around every handler body it dispatches, so ordinary
/// application code inherits this without asking. The guard restores the previous state when it
/// is dropped, so zones nest.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, enter_non_reactive_zone, install};
///
/// install().unwrap();
/// let node = Mounted::new();
/// let count = node.with(|| RwSignal::new(1));
///
/// let doubled = {
///     let _zone = enter_non_reactive_zone();
///     count.get() * 2 // deliberately a snapshot, not a dependency
/// };
/// assert_eq!(doubled, 2);
/// node.unmount();
/// ```
#[must_use = "the zone ends as soon as the guard is dropped"]
pub fn enter_non_reactive_zone() -> NonReactiveZone {
    NonReactiveZone {
        _guard: reactive_graph::diagnostics::SpecialNonReactiveZone::enter(),
    }
}
