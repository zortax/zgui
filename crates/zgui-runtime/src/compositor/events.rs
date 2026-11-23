//! What the driver told one window since the window last asked.

use compositor_api::{ClientEvent, CommitReport, WatchCause};
use compositor_types::{
    CommitError, CompositorEvent, FrameCounters, FrameOutcome, FrameStats, GpuUnavailable,
    ScrollGesture, ScrollId, ScrollRouting, SkipReason,
};

/// The answers and events the driver queued for a window, in the order they happened.
#[derive(Debug, Default)]
pub(crate) struct Answers {
    /// The compositor's own events, such as the offsets its scrolling shows.
    pub(crate) compositor: Vec<CompositorEvent>,
    /// The reports of the commits that applied.
    pub(crate) committed: Vec<CommitReport>,
    /// The first rejection, if a commit was rejected.
    pub(crate) rejected: Option<CommitError>,
    /// The gestures the compositor routed otherwise than the window predicted, with the routing
    /// it chose.
    pub(crate) rerouted: Vec<(ScrollGesture, ScrollRouting)>,
    /// The scroll watches that fired, with the node, the offset it shows and its motion step.
    pub(crate) watched: Vec<(ScrollId, WatchCause, [f32; 2], [f32; 2])>,
    /// The frames drawn for the window's output, with their counters and outcomes.
    pub(crate) drawn: Vec<(FrameCounters, FrameOutcome)>,
    /// Whether the output presented its first frame since the window watched for it.
    pub(crate) shown: bool,
    /// The newest presenter state of the output: whether it is starved, and whether it is hidden.
    pub(crate) presenter: Option<(bool, bool)>,
    /// Why the output's renderer could not be built, if it could not.
    pub(crate) failed: Option<GpuUnavailable>,
    /// Whether a commit was queued that no frame drawn yet holds.
    pub(crate) awaiting: bool,
}

impl Answers {
    /// Sorts `events` into answers, leaving it empty.
    pub(crate) fn of(events: &mut Vec<ClientEvent>) -> Self {
        let mut answers = Self::default();
        for event in events.drain(..) {
            match event {
                ClientEvent::Compositor(event) => answers.compositor.push(event),
                ClientEvent::Committed { report, .. } => answers.committed.push(report),
                ClientEvent::Rejected { error, .. } => {
                    answers.rejected.get_or_insert(error);
                }
                ClientEvent::Rerouted {
                    gesture, routing, ..
                } => answers.rerouted.push((gesture, routing)),
                ClientEvent::Watched {
                    node,
                    cause,
                    offset,
                    step,
                    ..
                } => answers.watched.push((node, cause, offset, step)),
                ClientEvent::Shown { .. } => answers.shown = true,
                ClientEvent::PresenterChanged {
                    starved, occluded, ..
                } => answers.presenter = Some((starved, occluded)),
                ClientEvent::OutputFailed { error, .. } => answers.failed = Some(error),
                ClientEvent::Drawn {
                    counters, outcome, ..
                } => answers.drawn.push((counters, outcome)),
                // An answer this build has never heard of asks nothing of the window.
                _ => {}
            }
        }
        answers
    }
}

/// What the frames drawn for a window's output since it last asked did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    /// What the last frame drawn reported.
    ///
    /// With no frame drawn, the window's work was handed over when a commit awaits its frame,
    /// and otherwise there was nothing to draw.
    pub(crate) outcome: FrameOutcome,
    /// Whether a frame was drawn.
    pub(crate) built: bool,
}

impl Drawn {
    /// Returns what the frames in `answers` did.
    pub(crate) fn of(answers: &Answers) -> Self {
        match answers.drawn.last() {
            Some((_, outcome)) => Self {
                outcome: *outcome,
                built: true,
            },
            None if answers.awaiting => Self {
                outcome: FrameOutcome::Presented(FrameStats::default()),
                built: false,
            },
            None => Self {
                outcome: FrameOutcome::Skipped(SkipReason::Undamaged),
                built: false,
            },
        }
    }
}
