//! The park, checked against a model of this loop's own turn.
//!
//! The state machine itself is [`zgui_platform::Park`]: every backend parks by the same three
//! lines of arithmetic, so they are stated once, in the contract, rather than per backend. What
//! lives here is the evidence that *this* adapter routes through it — a model of the turn this
//! loop takes, driven with deliberately broken readings of the park as positive controls, and a
//! soak over the race between the two clock readings the adapter has between asking the
//! application what it wants and installing the answer.

pub use zgui_platform::{Install, Park, Parked};

#[cfg(test)]
mod model;
#[cfg(test)]
mod soak;
#[cfg(test)]
mod tests;
