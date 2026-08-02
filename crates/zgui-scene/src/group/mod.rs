//! Groups: content that has to be composited as a unit, and what it reads to do so.

pub mod backdrop;
pub mod boundary;
pub mod filter;
pub mod source;

#[cfg(test)]
mod tests;

pub use crate::group::backdrop::BackdropFilter;
pub use crate::group::boundary::GroupBoundary;
pub use crate::group::filter::Filter;
pub use crate::group::source::read_extent;
