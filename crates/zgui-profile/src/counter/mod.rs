//! The per-frame counter block: what a frame actually did, in numbers a test can assert on.
//!
//! Every counter records work *performed*. That makes them the instrument budget tests are
//! written against: "hovering one row of a thousand restyles the row and its ancestors and
//! nothing else" is a sentence about [`Counter::ElementsRestyled`], and without the counter it is
//! a hope rather than a test.
//!
//! ```
//! use zgui_profile::{Counter, counter};
//!
//! counter::reset();
//! counter::bump(Counter::ElementsRestyled);
//! counter::add(Counter::SelectorMatches, 12);
//!
//! let frame = counter::snapshot();
//! if zgui_profile::COUNTERS_ENABLED {
//!     assert_eq!(frame.elements_restyled, 1);
//!     assert_eq!(frame.selector_matches, 12);
//! }
//! ```

mod define;
mod exclusive;
mod group;
pub mod non_vacuity;
mod store;
mod table;

#[cfg(test)]
mod tests;

pub use crate::counter::exclusive::exclusive;
pub use crate::counter::group::Group;
pub use crate::counter::store::{COUNTERS_ENABLED, add, bump, get, reset, set, snapshot};
pub use crate::counter::table::{Counter, Counters};
