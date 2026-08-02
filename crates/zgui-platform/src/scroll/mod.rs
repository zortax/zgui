//! What a scroll from a wheel or a touch surface *means* on the desktop this program is on.
//!
//! A scroll event carries a number and a unit, and neither of those is enough to move a document
//! correctly. How far one detent of a notched wheel is meant to travel, whether the number has
//! already had the person's scroll-direction preference applied to it, and whether the platform is
//! going to animate the detent itself are all facts about the desktop — and every one of them is
//! answered differently by the desktops this framework has to run on.
//!
//! Answering them with constants is how a wheel comes to feel wrong. A constant is right on the
//! machine it was written on and wrong everywhere else, and it is never *obviously* wrong: the
//! document does move, in some direction, by some amount, so nothing fails and nobody can point at
//! a line. This module is where those answers are stated instead, once, by whoever actually knows
//! them — the backend.
//!
//! | Module | What it settles |
//! |---|---|
//! | [`direction`] | whether the framework must apply the person's preference or the desktop already did |
//! | [`motion`] | whether a detent arrives whole or as a stream the platform is already animating |
//! | [`elastic`] | whether an edge follows a pull past the end, and for which inputs |
//! | [`settings`] | the four answers together, as one value a backend hands over |
//!
//! # The sign convention
//!
//! Stated once, here, and depended on everywhere:
//!
//! **A positive delta moves the scroll offset right and down** — it reveals content further right
//! and further down, exactly as a positive `scrollTop` does. The content itself therefore moves
//! *up and left* on the screen.
//!
//! That is deliberately the opposite of the convention several windowing libraries use, which
//! describe the movement of the *content* rather than of the offset. A backend converts into the
//! convention above before its events leave it, and says so in its own tests, because a convention
//! that is written down in one crate and assumed in another is a convention that survives until
//! somebody reads only the second one.

pub mod direction;
pub mod elastic;
pub mod motion;
pub mod settings;

pub use crate::scroll::direction::ScrollDirection;
pub use crate::scroll::elastic::Elastic;
pub use crate::scroll::motion::WheelMotion;
pub use crate::scroll::settings::ScrollSettings;
