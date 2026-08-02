//! A node tree that writes down everything it is asked to do.

mod editing;
mod handlers;
mod recording;

pub use crate::dom::editing::Edited;
pub use crate::dom::handlers::{Handler, Handlers, Registration};
pub use crate::dom::recording::RecordingDom;
