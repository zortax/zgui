//! Browser input events in the framework's vocabulary.
//!
//! | Module | Content |
//! |---|---|
//! | [`pointer`] | mouse, touch and pen as one pointer stream |
//! | [`keyboard`] | keys, their positions and the held modifiers |
//! | [`wheel`] | wheel events as detents or as a continuous stream |

pub mod keyboard;
#[cfg(target_arch = "wasm32")]
pub mod pointer;
pub mod wheel;
