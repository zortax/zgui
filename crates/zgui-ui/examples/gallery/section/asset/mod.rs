//! The vector documents these panels draw, as source text.
//!
//! They are `&'static str` rather than files because that is what a `<vector>` carries: the
//! document itself crosses the seam, not a name to go and fetch. An application with assets on disk
//! reads them once and hands over the same thing.

pub(crate) mod colour;
pub(crate) mod mono;
