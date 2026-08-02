//! Keyboard input, in the three descriptions a handler may need.

mod code;
mod event;
mod named;

pub use crate::event::payload::key::code::{KeyCode, PhysicalKey, UnknownKeyCode};
pub use crate::event::payload::key::event::{Key, KeyEvent, KeyLocation, KeyState};
pub use crate::event::payload::key::named::{NamedKey, UnknownNamedKey};
