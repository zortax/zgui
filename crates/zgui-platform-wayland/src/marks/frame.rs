//! The marks of one surface.
//!
//! The input handlers add the ids of the events that they processed. A redraw seals the ids that
//! arrived before it began, because the frame it builds contains their effect. The present of
//! that frame sends the sealed ids, the frame number and the phases. The marks apply on the
//! commit that the present makes, because they precede it on the connection.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use fakeland_marks::{InputEventId, SurfaceMarks};

use crate::marks::clock::Anchor;
use crate::marks::phases;

/// How many latency marks a present reads back. A frame writes about a hundred.
const READ_BACK: usize = 512;

/// The span from the first handled input event to the redraw that handles it.
const INPUT_WAIT: &str = "input_wait";

/// The surface mark on the first presented frame.
const FIRST_FRAME: &str = "first_frame";

/// The marks of one surface. Empty until the connection attaches the marks object.
#[derive(Debug, Default)]
pub(crate) struct FrameMarks {
    /// The state, present only when the compositor advertised the marks global.
    state: OnceLock<Mutex<Frames>>,
}

/// The state of an attached surface.
#[derive(Debug)]
struct Frames {
    /// The marks object of the surface.
    marks: SurfaceMarks,
    /// The clock conversion of the connection.
    anchor: Anchor,
    /// Ids of processed events that no redraw sealed yet.
    pending: Vec<InputEventId>,
    /// When the oldest pending id arrived.
    pending_since: Option<Instant>,
    /// Ids that the next present sends.
    sealed: Vec<InputEventId>,
    /// When the oldest sealed id arrived.
    sealed_since: Option<Instant>,
    /// When the latest redraw began.
    began: Option<Instant>,
    /// How many frames the surface presented.
    frames: u64,
}

impl FrameMarks {
    /// Marks with nothing attached.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Attaches the marks object of the surface.
    pub(crate) fn attach(&self, marks: SurfaceMarks, anchor: Anchor) {
        let _ = self.state.set(Mutex::new(Frames {
            marks,
            anchor,
            pending: Vec::new(),
            pending_since: None,
            sealed: Vec::new(),
            sealed_since: None,
            began: None,
            frames: 0,
        }));
    }

    /// Records the ids of input events that a handler processed at `at`.
    pub(crate) fn handled(&self, ids: Vec<InputEventId>, at: Instant) {
        let Some(mut frames) = self.lock() else {
            return;
        };
        frames.pending_since.get_or_insert(at);
        frames.pending.extend(ids);
    }

    /// Seals the processed ids for the frame that a redraw beginning at `began` builds.
    pub(crate) fn redraw(&self, began: Instant) {
        let Some(mut frames) = self.lock() else {
            return;
        };
        let frames = &mut *frames;
        if let Some(since) = frames.pending_since.take() {
            frames.sealed_since.get_or_insert(since);
        }
        frames.sealed.append(&mut frames.pending);
        frames.began = Some(began);
    }

    /// Sends the marks of the frame that presents now, ahead of the commit of the present.
    pub(crate) fn present(&self, now: Instant) {
        let Some(mut frames) = self.lock() else {
            return;
        };
        let frames = &mut *frames;
        frames.frames += 1;
        frames.marks.frame(frames.frames);
        for id in frames.sealed.drain(..) {
            frames.marks.input_handled(id);
        }
        let waited = frames.sealed_since.take();
        if let Some(began) = frames.began {
            if let Some(since) = waited.filter(|since| *since <= began) {
                span(frames, INPUT_WAIT, since, began);
            }
            for phase in phases::extract(&recorded(), began, now) {
                span(frames, phase.name, phase.begin, phase.end);
            }
        }
        if frames.frames == 1 {
            // The name is a valid constant, so the call cannot fail.
            let _ = frames.marks.mark(FIRST_FRAME);
        }
    }

    /// Drops the marks object, ahead of the surface it belongs to.
    pub(crate) fn release(&mut self) {
        drop(self.state.take());
    }

    /// The state, when the surface has marks.
    fn lock(&self) -> Option<MutexGuard<'_, Frames>> {
        let state = self.state.get()?;
        Some(
            state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }
}

/// Sends one span. The phases are in order and the names are valid constants, so the call cannot
/// fail.
fn span(frames: &Frames, name: &str, begin: Instant, end: Instant) {
    let _ = frames.marks.span(
        name,
        frames.anchor.timestamp(begin),
        frames.anchor.timestamp(end),
    );
}

/// The latency marks still in the ring, as instants.
fn recorded() -> Vec<(&'static str, Instant)> {
    let Some(epoch) = zgui_profile::latency::epoch() else {
        return Vec::new();
    };
    zgui_profile::latency::last(READ_BACK)
        .into_iter()
        .map(|mark| {
            let after = u64::try_from(mark.at_ns).unwrap_or(u64::MAX);
            (mark.stage, epoch + std::time::Duration::from_nanos(after))
        })
        .collect()
}
