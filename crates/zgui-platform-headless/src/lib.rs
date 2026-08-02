//! A platform with no windowing system behind it: a clock a test moves, scripted input, and a
//! surface that is a buffer.
//!
//! Everything an application does through the platform contract it can do here, deterministically
//! and with no display server. That is worth having for its own sake — most of a user interface
//! can be exercised without one — but the reason this backend is shipped rather than kept in a
//! test module is the frame loop's **parking**, which is the hardest part of the loop to get right
//! and the only part whose two failure modes both look like nothing happening.
//!
//! A deadline that is never turned back into a request to draw is a *stall*: a timer that fires no
//! frame. A deadline installed when it has already passed is a *spin*: the platform reports it
//! reached on every turn of the loop, for ever, and the loop runs no frames while burning a core.
//! Only the ratio of resumes to frames tells the second from a correct park, and
//! [`Harness::assert_park_invariant`] is what checks it.
//!
//! ```
//! use std::time::Duration;
//! use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent,
//!     SurfaceId, WakeReason};
//! use zgui_platform_headless::Harness;
//!
//! /// An application that asks to be woken once, seven hundred milliseconds from now.
//! #[derive(Default)]
//! struct Delayed {
//!     deadline: Option<std::time::Instant>,
//!     frames: u32,
//! }
//!
//! impl AppHandler for Delayed {
//!     fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
//!         cx.create_surface(&SurfaceAttributes::new("delayed")).expect("headless");
//!         self.deadline = Some(cx.clock().now() + Duration::from_millis(700));
//!     }
//!
//!     fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
//!         if matches!(event, SurfaceEvent::RedrawRequested) {
//!             self.frames += 1;
//!             self.deadline = None;
//!         }
//!     }
//!
//!     fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
//!
//!     fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
//!         self.deadline.map_or(IdlePolicy::Block, |at| IdlePolicy::until(at, cx.clock().now()))
//!     }
//!
//!     fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
//!         for surface in cx.surfaces() {
//!             surface.request_redraw();
//!         }
//!     }
//! }
//!
//! let mut harness = Harness::new(Delayed::default());
//! harness.pump();
//! assert!(harness.parked_deadline().is_some());
//!
//! harness.advance(Duration::from_millis(700));
//! assert_eq!(harness.redraws_requested(), 1, "the deadline itself asked for the frame");
//!
//! harness.pump();
//! assert_eq!(harness.frames_requested(), 1);
//! assert!(harness.parked_deadline().is_none(), "an expired deadline is never re-installed");
//! ```
//!
//! # What this is not
//!
//! It is never a silent fallback for a real window. An application that asked for a window and got
//! a buffer is worse off than one that was told no: the window appears in no task bar, nothing is
//! ever presented, and the failure is invisible until someone asks why the screen is empty. A
//! windowing backend that cannot open a device reports that; it does not reach for this.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod clipboard;
pub mod harness;
pub mod platform;
pub mod surface;
pub mod waker;

pub use crate::clipboard::MemoryClipboard;
pub use crate::harness::Harness;
pub use crate::platform::Headless;
pub use crate::surface::OffscreenSurface;
pub use crate::waker::RecordingWaker;

/// The clock a caller moves by hand, re-exported.
///
/// It lives with the platform contract rather than here because a test harness that runs frames by
/// hand needs one too and arrives long before this backend does — but it is the clock [`Headless`]
/// runs on, so it is named here as well. There is exactly one implementation, so the backend and
/// the harness cannot drift apart in how time behaves.
pub use zgui_platform::VirtualClock;
