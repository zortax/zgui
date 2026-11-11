//! Frame and input marks for the fakeland compositor.
//!
//! When the compositor advertises `fakeland_marks_manager_v1`, each committed frame of a surface
//! carries its frame number, the compositor ids of the input events that the frame handled, and
//! the phases that built it. The phases come from the latency marks of the frame pipeline. The
//! connection also marks the start-up steps: `connected`, `configured` and, per surface,
//! `first_frame`.
//!
//! Without the global every hook returns after one check, and the latency ring stays off.
//!
//! | Module | Contents |
//! |---|---|
//! | `app` | the marks of the connection, and the input hook of the handlers |
//! | `frame` | the marks of one surface, sealed when a redraw starts and sent before its present |
//! | `phases` | the frame phases, read from the latency marks |
//! | `clock` | the conversion of an instant to a monotonic timestamp |

mod app;
mod clock;
mod frame;
mod phases;

pub(crate) use crate::marks::app::AppMarks;
pub(crate) use crate::marks::frame::FrameMarks;
pub(crate) use fakeland_marks::Family;
