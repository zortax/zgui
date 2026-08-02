//! Motion that is not CSS: springs and tweens over plain numbers.
//!
//! Everything else in this crate drives the style system. This drives nothing at all — it advances
//! a number, and a component writes that number into a signal. Nothing here enters the cascade, so
//! a gesture that is being followed frame by frame costs no style work whatever.
//!
//! It is the opt-in path and not the default. A component that can express its motion in a
//! stylesheet should, because then a designer can change it; this is for the motion a stylesheet
//! cannot express — a value that follows a finger, an exit whose velocity is whatever the drag left
//! behind.
//!
//! | Module | Contents |
//! |---|---|
//! | [`spring`] | a critically damped mass on a spring, driven by its own physics |
//! | [`tween`] | a value moved from one number to another over a fixed duration |

pub mod spring;
pub mod tween;

pub use crate::motion::spring::Spring;
pub use crate::motion::tween::{Easing, Tween};
