//! What an assistive technology is told about one line of text.
//!
//! A screen reader navigating by character, a magnifier tracking the caret and a braille display
//! all need the same four things about a run of text: where each character starts, how wide it is,
//! how many bytes it occupies, and where the words begin. None of those can be derived from the
//! string — a character is the smallest unit that can be *selected*, which depends on how the text
//! was shaped — so they come from here, where the shaping is known.

pub mod run;

pub use crate::a11y::run::{ClusterGeometry, TextRunAttributes};
