//! The check that the compositor changed nothing the paint walk did not visit.
//!
//! Every pixel a compositor frame changes because a chunk was recorded, removed or reordered lies
//! where the walk was gated on, because the walk only records what it visits. The compositor also
//! changes pixels for reasons the walk never sees, so a frame is checked only when it has none of
//! them: no property, paint or offset write in its commits, no compositor scrolling, no shift, a
//! frame before it that retired its damage, and changes short of the whole output.
//!
//! The frames are checked where the driver builds them, and a violation is reported to the window
//! when it next takes the driver's answers.

mod cover;

use std::sync::{Arc, Mutex, PoisonError};

use compositor_types::Frame;
use zgui_bits::DamageSet;
use zgui_geom::{Point, Rect, Size};

use crate::compositor::Client;

/// The commits a window owes the containment check, shared with the observer of its output's
/// frames.
#[derive(Clone, Debug)]
pub(crate) struct Containment(Arc<Mutex<Expected>>);

/// What the next frame built is checked against.
#[derive(Debug)]
struct Expected {
    /// The pixels the paint walks of the commits since the last frame visited.
    painted: DamageSet,
    /// Whether a commit arrived since the last frame.
    committed: bool,
    /// Whether every commit since the last frame is fit for the check.
    fit: bool,
    /// Whether the last frame drawn retired its damage.
    retired: bool,
    /// The first violation found, until the window takes it.
    violation: Option<String>,
}

impl Containment {
    /// Starts checking the frames of the output `client` draws into.
    pub(crate) fn observe(client: &Client) -> Self {
        let containment = Self(Arc::new(Mutex::new(Expected {
            painted: DamageSet::new(),
            committed: false,
            fit: true,
            retired: false,
            violation: None,
        })));
        let observed = containment.clone();
        client.observe_frames(Some(Box::new(move |frame| observed.built(frame))));
        containment
    }

    /// Adds a commit whose paint walk visited `painted` to the next frame's check, which the
    /// commit leaves fit when `fit` holds.
    pub(crate) fn committed(&self, painted: &DamageSet, fit: bool) {
        let mut expected = self.lock();
        expected.painted.absorb_set(painted);
        expected.committed = true;
        expected.fit &= fit;
    }

    /// Notes whether the frame drawn last retired its damage.
    pub(crate) fn drawn(&self, retired: bool) {
        self.lock().retired = retired;
    }

    /// Panics when a frame built since the last call redrew a pixel no paint walk visited.
    pub(crate) fn assert_contained(&self) {
        if let Some(violation) = self.lock().violation.take() {
            panic!("{violation}");
        }
    }

    /// Checks `frame` against the commits since the frame before it.
    fn built(&self, frame: &Frame) {
        let mut expected = self.lock();
        let fit = std::mem::replace(&mut expected.fit, true);
        let painted = std::mem::take(&mut expected.painted);
        if !std::mem::take(&mut expected.committed) || !fit || !expected.retired {
            return;
        }
        if expected.violation.is_none() {
            expected.violation = violation(frame, &painted);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Expected> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Describes the pixels `frame` changes that `painted` does not hold, if there are any.
///
/// The changed pixels are those that differ from the previous image presented. A frame that
/// shifted or changes the whole output has none.
fn violation(frame: &Frame, painted: &DamageSet) -> Option<String> {
    // An image older than the last one is drawn over its older damage too, which no walk of this
    // frame visited.
    let draw = &frame.damage.present;
    if !frame.shifts.is_empty() || draw.is_full() || painted.is_full() {
        return None;
    }
    draw.rects().iter().find_map(|drawn| {
        let drawn = Rect::new(
            Point::new(drawn.origin.x, drawn.origin.y),
            Size::new(drawn.size.width, drawn.size.height),
        );
        let outside = cover::uncovered(drawn, painted.rects())?;
        Some(format!(
            "the compositor changed {outside:?}, which the paint walk never visited: changed {:?}, \
             painted {:?}",
            draw.rects(),
            painted.rects()
        ))
    })
}
