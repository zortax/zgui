//! Geometry delivered to a view as it changes.

use core::fmt::{self, Debug};
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Rect, Size};

use crate::scroll::ScrollPosition;

/// A geometric quantity a view can ask to be told about.
///
/// Deliberately a small closed set. Observation costs a slot in the frame's geometry diff, so what
/// can be observed is what a component library genuinely needs — a popover following its anchor, a
/// virtualised list following a scroll offset, a container query following its own size — and not
/// an open door onto the layout tree.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Observed {
    /// The node's border box, in device pixels, relative to the window.
    BorderBox,
    /// The size of the node's content area.
    ContentSize,
    /// The node's scroll offset, content extent and visible extent.
    ScrollPosition,
}

/// One delivered observation.
///
/// The variant always matches the [`Observed`] the registration named, so a sink can extract what
/// it asked for with one of the accessors and treat a mismatch as a backend bug rather than as a
/// case to handle.
#[derive(Copy, Clone, PartialEq, Debug)]
#[non_exhaustive]
pub enum ObservedValue {
    /// A border box.
    BorderBox(Rect<DevicePx, Device>),
    /// A content size.
    ContentSize(Size<DevicePx, Device>),
    /// A scroll position.
    ScrollPosition(ScrollPosition),
}

impl ObservedValue {
    /// Which quantity this value answers.
    pub const fn observed(self) -> Observed {
        match self {
            Self::BorderBox(_) => Observed::BorderBox,
            Self::ContentSize(_) => Observed::ContentSize,
            Self::ScrollPosition(_) => Observed::ScrollPosition,
        }
    }

    /// The border box, when this is one.
    pub const fn as_border_box(self) -> Option<Rect<DevicePx, Device>> {
        match self {
            Self::BorderBox(rect) => Some(rect),
            _ => None,
        }
    }

    /// The content size, when this is one.
    pub const fn as_content_size(self) -> Option<Size<DevicePx, Device>> {
        match self {
            Self::ContentSize(size) => Some(size),
            _ => None,
        }
    }

    /// The scroll position, when this is one.
    pub const fn as_scroll_position(self) -> Option<ScrollPosition> {
        match self {
            Self::ScrollPosition(position) => Some(position),
            _ => None,
        }
    }
}

/// Where an observation is delivered.
///
/// One boxed call per delivery, which is what lets a backend answer the registration with whatever
/// it already has — a slot in the frame's geometry diff, a `ResizeObserver`, a scroll event — and
/// call back into the view layer without knowing anything about it.
pub type ObservationSink = Rc<dyn Fn(ObservedValue)>;

/// Keeps one observation alive.
///
/// Dropping it deregisters. The backend decides what that costs it; from a view's side the
/// contract is only that after the drop returns, no further value is delivered.
#[must_use = "dropping the handle stops the observation"]
pub struct ObservationHandle {
    /// Run once, on drop.
    release: Option<Box<dyn FnOnce()>>,
}

impl ObservationHandle {
    /// Builds a handle whose drop runs `release`.
    pub fn new(release: impl FnOnce() + 'static) -> Self {
        Self {
            release: Some(Box::new(release)),
        }
    }

    /// A handle over an observation that was never really registered.
    ///
    /// For a backend that answers a quantity it cannot change — a fixed-geometry test host, a
    /// document that is not laid out — where there is nothing to deregister.
    pub fn inert() -> Self {
        Self { release: None }
    }
}

impl Debug for ObservationHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObservationHandle")
            .field("registered", &self.release.is_some())
            .finish()
    }
}

impl Drop for ObservationHandle {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{ObservationHandle, Observed, ObservedValue};
    use crate::scroll::ScrollPosition;

    #[test]
    fn dropping_a_handle_deregisters_exactly_once() {
        let releases = Rc::new(Cell::new(0));
        {
            let releases = Rc::clone(&releases);
            let _handle = ObservationHandle::new(move || releases.set(releases.get() + 1));
            assert_eq!(Rc::strong_count(&Rc::new(())), 1);
        }
        assert_eq!(releases.get(), 1);
    }

    #[test]
    fn an_inert_handle_runs_nothing() {
        let handle = ObservationHandle::inert();
        drop(handle);
    }

    #[test]
    fn a_value_reports_the_quantity_it_answers() {
        let value = ObservedValue::ScrollPosition(ScrollPosition::default());
        assert_eq!(value.observed(), Observed::ScrollPosition);
        assert!(value.as_scroll_position().is_some());
        assert!(value.as_border_box().is_none());
    }
}
