//! Imperative properties: the values that are neither attributes nor selector-visible states.

pub mod drawing;
mod key;
mod value;

pub use crate::prop::key::PropKey;
pub use crate::prop::value::PropValue;
