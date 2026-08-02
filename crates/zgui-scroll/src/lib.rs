//! Scrolling as state: offsets, chaining, elastic overscroll, momentum and smooth motion.
//!
//! Layout owns scroll *regions* — which boxes scroll, how far their content reaches, where the
//! scrollport is. This crate owns scroll *offsets*, and the distinction is the whole design.
//! An offset changes many times a second while a wheel turns or a finger moves, and if changing one
//! re-entered the cascade or the layout engine then scrolling would cost what a resize costs. It
//! does not: writing an offset marks one bit on one element, and the fragment pass composes the new
//! absolute positions on the way past. Nothing above that pass runs.
//!
//! | Module | What it owns |
//! |---|---|
//! | [`scroller`] | the offsets themselves, everything that writes one, and what one owes when the extent under it moves |
//! | [`chain`] | how a delta is shared out along a chain of nested containers |
//! | [`elastic`] | what happens to a delta that no container could absorb |
//! | [`motion`] | offsets that move over time: a smooth scroll and a flung one |
//! | [`into_view`] | the offset that brings a rectangle into a scrollport |
//! | [`mark`] | what a container that moved owes the rest of the frame |
//! | [`report`] | what a scroll tells a listener |
//!
//! ```
//! use zgui_scroll::Scroller;
//!
//! let scroller = Scroller::new();
//! assert!(!scroller.is_animating(), "nothing is moving yet");
//! assert!(scroller.settled(), "and nothing is displaced past its end either");
//! ```
//!
//! # What this crate deliberately does not do
//!
//! **It never re-enters layout.** The only thing it reads from the layout store is the region an
//! offset is clamped against, which layout has already computed. It writes nothing there.
//!
//! **It runs no handler and knows no listener.** A scroll produces a [`report::Scrolled`] describing
//! what moved; turning that into an event a view hears is the runtime's, because a handler is a
//! view-layer value and this layer sits below one.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod chain;
pub mod elastic;
pub mod into_view;
pub mod mark;
pub mod motion;
pub mod report;
pub mod scroller;
pub mod stretch;

pub use crate::chain::Absorbed;
pub use crate::into_view::Align;
pub use crate::motion::Behavior;
pub use crate::report::Scrolled;
pub use crate::scroller::Scroller;
pub use crate::stretch::Stretch;
