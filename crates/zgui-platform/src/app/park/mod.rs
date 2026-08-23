//! How the loop parks, and the two ways of getting that wrong.
//!
//! Everything about waiting lives here, and nothing about waiting lives in any backend. That is
//! deliberate: parking is decided by three lines of arithmetic whose failure modes are invisible
//! from outside, so they are written once, in a type with no event loop behind it, and every
//! backend routes through the same one rather than re-deriving it.
//!
//! # The two failures
//!
//! **The stall.** A deadline arriving is reported as the *cause* of the next turn of the loop —
//! never as a request to draw. Nothing is drawn until something asks, so the arrival has to be
//! turned back into a request by hand. Miss that and a timer fires no frame, an animation never
//! advances, and the symptom is an application that ignores its own clock.
//!
//! **The spin.** A deadline that has already passed, installed anyway, is not a deadline that
//! fires late. The remaining time is recomputed from the installed instant on every turn, is found
//! to be zero every time, and the arrival is re-derived every time. The loop reports an expiry per
//! iteration, runs no frames, and burns a core. It looks exactly like the stall from outside.
//!
//! # One invariant covering both
//!
//! **A moment the application named is either waited for or handed over — never dropped.**
//!
//! [`Park::install`] never installs a moment that is not strictly in the future, which is the
//! defence against the spin. It never discards one either, which is the defence against a third
//! failure that the clamp on its own creates and that this project has now shipped six times.
//!
//! The third failure is a race, and it is the reason the invariant is stated over the moment
//! rather than over the clock. The application decides what it wants against one reading of the
//! clock; the loop installs it against a second reading, microseconds later. A moment picked four
//! microseconds ahead is in the future for the first reading and in the past for the second, and a
//! clamp that answers "expired, so park on nothing" has just blocked the loop for ever while
//! holding a frame somebody is waiting for. It is not reachable by inspection — it needs the two
//! readings to straddle the moment — and it is reachable in practice roughly once in four hundred
//! thousand turns.
//!
//! So [`Park::install`] returns an [`Install`], not a park. A moment that has passed comes back as
//! [`Install::Overdue`], and the only route from there to a [`Parked`] is [`Install::park`], which
//! takes the delivery as an argument. Forgetting to look at a flag, an early return and a `match`
//! arm added later all fail to compile rather than fail to draw. The obligation cannot be dropped;
//! it can only be discharged.
//!
//! [`Park::resumed`] is the edge for the stall proper, and it clears the deadline *before* the
//! application is told, so a handler that installs a fresh one from inside its own callback is not
//! overwritten by the clearing that follows it.
//!
//! # What "did not spin" is measured as
//!
//! Not processor time, which no test can assert on portably, but the ratio the two failures move
//! in opposite directions: **expiries reported must not exceed frames run, plus the one that has
//! been reported and whose frame has not run yet.** A correct park keeps it; a spin breaks it
//! immediately and loudly. [`Park::resumes`] is the numerator.

mod install;
mod policy;

pub use crate::app::park::install::Install;
pub use crate::app::park::policy::{Park, Parked};
