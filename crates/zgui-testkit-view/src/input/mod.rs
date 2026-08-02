//! Input a test synthesises, delivered the way a running window delivers it.
//!
//! A component is not tested by calling its handlers. It is tested by pressing it: an event aimed
//! at a point, resolved against the geometry the test declared, delivered down to the element it
//! landed on and back up again, with everything a handler says about it honoured. Anything less
//! than that misses the failures that matter — a listener registered twice, an overlay that never
//! sees the press outside it, a handler that stops propagation and a sibling that runs anyway.
//!
//! The order handlers run in is not decided here. It comes from the same rule the real document
//! resolves against, so a component that behaves under this harness behaves in a window for the
//! same reason rather than by coincidence.

mod dispatch;
mod hit;
mod sink;

pub use crate::input::dispatch::{Delivered, Dispatcher};
pub use crate::input::hit::topmost;
pub use crate::input::sink::{Command, Commands};
