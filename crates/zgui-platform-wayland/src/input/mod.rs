//! Input, as it arrives from the compositor.
//!
//! Every module here answers one question: what a key means ([`keyboard`]), where a pointer is and
//! what it did ([`pointer`]), what a finger did ([`touch`]), what is being dragged over a window
//! ([`dnd`]), which devices a person has ([`seat`]), and which serial a request may quote
//! ([`serial`]).

pub mod dnd;
pub mod keyboard;
pub mod pointer;
pub mod scrolling;
pub mod seat;
pub mod serial;
pub mod text_input;
pub mod touch;

pub use crate::input::dnd::Drag;
pub use crate::input::scrolling::desktop_scroll_settings;
pub use crate::input::seat::Seat;
pub use crate::input::serial::Serials;
