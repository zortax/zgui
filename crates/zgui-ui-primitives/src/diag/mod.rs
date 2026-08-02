//! A trace of what the dismissal, presence and focus machinery did, for a defect that only appears
//! on a real compositor.
//!
//! A modal that occasionally refuses to unmount cannot be caught by an assertion, because the run
//! that catches it is the run that ends the session: the surface stays up, its focus trap stays
//! installed, its layer stays on the stack, and nothing after it can be driven. What is needed
//! instead is a record of the decisions that led there — which element the exit listeners were
//! bound to, whether an end ever arrived and at which node, what the deferred check saw, and which
//! layer answered the Escape.
//!
//! So every one of those decisions writes one line to standard error, and only when
//! `ZGUI_MODAL_TRACE` is set in the environment. Lines are stamped in microseconds from the first
//! one, and every line begins `ZMT ` so that a session's output can be separated from whatever else
//! the process says.
//!
//! ```text
//! ZMT 41822 presence.end kind=animation surface=Some(NodeId(41)) at=Some(NodeId(41)) running=0
//! ```

mod clock;
mod switch;

/// Writes one trace line, building its fields only when the trace is on.
///
/// The fields are a format string, so a call site that would have to allocate to describe itself
/// pays nothing in a run with the trace off.
macro_rules! note {
    ($kind:literal $(, $($arg:tt)*)?) => {
        if $crate::diag::enabled() {
            $crate::diag::line($kind, &format!($($($arg)*)?));
        }
    };
}

pub(crate) use note;

/// Whether trace lines are being written.
pub(crate) fn enabled() -> bool {
    switch::on()
}

/// Writes one line. [`note!`] is what call sites use.
pub(crate) fn line(kind: &str, fields: &str) {
    eprintln!("ZMT {} {kind} {fields}", clock::micros());
}

thread_local! {
    /// The next number [`next_id`] hands out.
    static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

/// A number that names one instance of a component for the length of a trace.
///
/// A node handle is not enough on its own: the whole question is what happens when the element
/// under a handle is replaced, so the lines about one mounting have to be separable from the lines
/// about the next.
pub(crate) fn next_id() -> u64 {
    NEXT.with(|held| {
        let id = held.get();
        held.set(id + 1);
        id
    })
}
