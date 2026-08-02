//! Events: which one happened, what it carried, and what a handler may say about it.

pub mod payload;
pub mod route;

mod control;
mod kind;
mod listener;

pub use crate::event::control::{DefaultAction, Propagation};
pub use crate::event::kind::{EventKind, UnknownEventKind};
pub use crate::event::listener::{ListenerOptions, Phase};
pub use crate::event::payload::{Payload, PayloadKind};
pub use crate::event::route::{Listeners, Path, RouteStep, route};
