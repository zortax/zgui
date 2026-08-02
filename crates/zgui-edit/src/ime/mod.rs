//! Composition: the provisional text an input method is still deciding on.
//!
//! An input method turns several key presses into one piece of text. While it is doing so the
//! field shows text that is *not* part of its value, and the platform keeps delivering the keys
//! the input method did not consume — every one of them, on X11 and on Wayland alike. An editor
//! that acts on those keys moves the caret out from under the composition, and the commit that
//! follows lands wherever the caret went instead of where the provisional text is.
//!
//! So while a composition is running the range it occupies is authoritative and key events are
//! refused. That is a policy of this crate rather than a promise of the platform, and it is what
//! [`Composition`] exists to hold.

pub mod composition;

pub use crate::ime::composition::Composition;
