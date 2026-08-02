//! An end-to-end latency trace: every stage boundary on the path from a platform event to a
//! presented frame, stamped with a monotonic clock and written out as one JSON object per mark.
//!
//! This is deliberately *not* the [`Phase`](crate::Phase) taxonomy. A phase span says how long a
//! stage took; it says nothing about the wall time between one stage ending and the next
//! beginning, and nothing at all about the time an event spent waiting for a frame to be asked
//! for. Those gaps are what a person means when they say an application feels slow, so what is
//! recorded here is *instants*, not durations, and every gap between two consecutive instants is
//! therefore visible whether or not anyone thought to name the thing that filled it.
//!
//! Nothing is recorded unless `ZGUI_LATENCY` names a file to write to. When it does not, a mark
//! costs one relaxed atomic load.

//! # Two sinks, two readers
//!
//! [`start_epoch`] writes the marks to a file, for a reader outside the process: a run finishes and
//! somebody reads the trace. [`retain`] keeps the last few thousand of them in memory instead, for
//! a reader inside it — an inspector drawing the shape of the frame on screen cannot read a file it
//! is itself still writing. Either, both or neither may be on; a mark with neither costs a relaxed
//! load.
//!
//! [`trace_elements`] is a third switch and not a sink at all: it decides whether marks may be
//! written *per element* rather than per frame. Neither sink implies it, because the volume of a
//! per-element trace is the document's size rather than a constant, and a bounded ring given one
//! loses every frame boundary it was kept for.

mod elements;
pub mod ring;
mod sink;

pub use crate::latency::elements::{trace_elements, tracing_elements};
pub use crate::latency::ring::{Recorded, clear, last, recent, retain, retaining};
pub use crate::latency::sink::{flush, mark, mark_at, marker, note, note_with, start_epoch};
