//! Turning what the machine reports into what a document dispatches.
//!
//! Every translation in this module is a *narrowing*: the windowing library reports what a device
//! did, and a document is told what a person did. The two are not the same vocabulary and the
//! difference is where the portability lives, so it is written down once, here, and checked.
//!
//! Three of the narrowings are load-bearing and each has its own module:
//!
//! * a key press answers three questions with three different answers — what to insert, which
//!   shortcut it is, and which physical position was pressed — and all three travel together
//!   ([`keyboard`]);
//! * a mouse, a finger and a stylus are one event stream told apart by a field, never three
//!   ([`mod@pointer`]);
//! * a scroll arrives in whichever unit the device measures in, and stays in it, because
//!   converting lines to pixels needs a line height that only the element being scrolled knows
//!   ([`wheel`]) — and it is turned around on the way through, because the windowing library
//!   describes the movement of the content and this framework describes the movement of the
//!   offset;
//! * how far one detent of a wheel is meant to travel, and who animates it, are properties of the
//!   desktop rather than of the event, so they are answered separately ([`scrolling`]).

pub mod ime;
pub mod keyboard;
pub mod pointer;
pub mod scrolling;
pub mod wheel;
