//! A real event loop, real windows, and the desktop this program is actually running on.
//!
//! This is the production implementation of the platform contract: the loop that blocks on the
//! compositor, the windows a person can drag, the keyboard and pointer as they arrive from the
//! hardware, the clipboards other applications can read, and the channel an assistive technology
//! talks to. Everything above it is written against the contract and never against this crate —
//! nothing here appears in any other manifest, and the only thing an application names is
//! [`run`].
//!
//! ```no_run
//! # struct Nothing;
//! # impl zgui_platform::AppHandler for Nothing {
//! #     fn surfaces_available(&mut self, _: &dyn zgui_platform::PlatformCx) {}
//! #     fn surface_event(
//! #         &mut self,
//! #         _: &dyn zgui_platform::PlatformCx,
//! #         _: zgui_platform::SurfaceId,
//! #         _: zgui_platform::SurfaceEvent,
//! #     ) {}
//! #     fn wake(&mut self, _: &dyn zgui_platform::PlatformCx, _: zgui_platform::WakeReason) {}
//! # }
//! zgui_platform_winit::run(Box::new(Nothing)).expect("the loop ran");
//! ```
//!
//! # Parking is the part that has to be right
//!
//! A user interface that is not changing must consume nothing, and one that has something to do in
//! seven hundred milliseconds must be woken then and not before. Both failures look identical from
//! outside — nothing happens — and they have opposite causes:
//!
//! * **the stall**: the loop parks on a deadline, is woken when it arrives, and nothing turns
//!   "the deadline arrived" into a request to draw. A reached deadline draws nothing by itself; it
//!   is reported as the *cause* of the next turn and never as a request to redraw, so the edge has
//!   to be closed by hand. Without it a timer fires no frame and an animation never advances.
//! * **the spin**: a deadline that has already passed is installed anyway. The time remaining is
//!   recomputed on every turn of the loop, is found to be zero every time, and the deadline is
//!   reported reached again — for ever. The loop runs no frames and burns a core.
//! * **the dropped moment**: the application picks a moment a few microseconds ahead, and it
//!   passes before the loop gets as far as installing it. Refusing to install it is right;
//!   forgetting it is a loop blocked with nothing to wake it, holding a frame somebody is waiting
//!   for. A moment that has passed is therefore *handed over* rather than discarded, and the type
//!   returned by the install is what makes handing it over the only thing that can be done with it.
//!
//! Both are handled in one place, [`park`], which is a plain state machine over
//! [`IdlePolicy`](zgui_platform::IdlePolicy). The event loop adapter holds no parking logic of its
//! own: it asks the state machine what to install and routes the expiry edge into it. That split is
//! what makes the four properties assertable against a model of the loop *and* against the loop
//! itself.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`app`] | the event loop adapter, and [`run`] |
//! | [`park`] | the park state machine, and the obligation an install can hand back |
//! | [`surface`] | one window: its handles and its chrome, and the loop's accessibility adapters |
//! | [`input`] | the translation of keyboard, pointer, wheel, input method and drag |
//! | [`clipboard`] | the desktop's clipboards, chosen at run time |
//! | [`clock`] | the monotonic clock every phase reads |
//! | [`waker`] | how another thread reaches a parked loop |
//! | [`cx`] | the borrowed context handed to every callback |
//! | [`monitor`] | what is known about the outputs |
//! | [`theme`] | the light or dark preference, where it can be discovered |

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for exactly one reason: the Wayland clipboard is
// constructed from a raw display pointer through an unsafe constructor. Every unsafe block states
// what makes it sound.
#![allow(unsafe_code)]

pub mod app;
pub mod clipboard;
pub mod clock;
pub mod cx;
pub mod input;
pub mod monitor;
pub mod park;
pub mod surface;
pub mod theme;
pub mod waker;

pub use crate::app::{WinitApp, event_loop, run};
pub use crate::clipboard::DesktopClipboard;
pub use crate::clock::SystemClock;
pub use crate::cx::WinitCx;
pub use crate::input::scrolling::desktop_scroll_settings;
pub use crate::park::{Install, Park, Parked};
pub use crate::surface::WinitSurface;
pub use crate::waker::{ProxyWaker, UserEvent};
