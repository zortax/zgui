//! Why something outside the surfaces asked for attention.

use accesskit::ActionRequest;

use crate::clipboard::{ClipboardData, ClipboardError, ClipboardSerial};
use crate::surface::SurfaceId;

/// Why the loop was woken by something that is not a surface event.
///
/// Every wake carries its reason, and the reasons are enumerated rather than opaque because what
/// happens next differs completely between them. Background work belonging to one window must not
/// redraw another; an accessibility request has to reach the document even when nothing is
/// visually dirty; a lost graphics device has to be rebuilt before anything is drawn at all.
/// A single unnamed "something happened" wake would collapse all of that into a full redraw of
/// everything, which is the behaviour this framework exists to avoid.
#[derive(Debug)]
#[non_exhaustive]
pub enum WakeReason {
    /// Background work made progress and the surfaces it belongs to may need redrawing.
    ///
    /// The surfaces are named because an image finishing its decode for one window is not a reason
    /// to redraw another.
    ReactiveWork {
        /// The surfaces the work belongs to.
        surfaces: Box<[SurfaceId]>,
    },
    /// An assistive technology asked for something to happen.
    ///
    /// These arrive from another thread on most platforms, which is exactly why they arrive as a
    /// wake rather than as a direct call.
    A11yAction(ActionRequest),
    /// An assistive technology attached and needs a complete tree before anything else.
    ///
    /// This has to force a build even when nothing is dirty: the tree has never been produced, so
    /// there is nothing for a dirty check to notice.
    A11yTreeRequested(SurfaceId),
    /// A clipboard read that was started earlier has finished.
    ClipboardRead {
        /// The read this answers.
        serial: ClipboardSerial,
        /// What was on the clipboard, or why it could not be read.
        result: Result<ClipboardData, ClipboardError>,
    },
    /// The graphics device was lost and everything built on it has to be rebuilt.
    DeviceLost,
    /// The desktop's light or dark preference changed.
    ColorSchemeChanged,
}

impl WakeReason {
    /// The surfaces this wake concerns, or an empty slice when it concerns all of them.
    ///
    /// An empty answer means "everything", because a lost device and a changed colour scheme are
    /// genuinely global; a non-empty one is a restriction the caller must honour.
    pub fn surfaces(&self) -> &[SurfaceId] {
        match self {
            Self::ReactiveWork { surfaces } => surfaces,
            Self::A11yTreeRequested(surface) => core::slice::from_ref(surface),
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WakeReason;
    use crate::surface::SurfaceId;

    #[test]
    fn work_is_confined_to_the_surfaces_it_belongs_to() {
        let reason = WakeReason::ReactiveWork {
            surfaces: Box::from([SurfaceId::new(7)]),
        };
        assert_eq!(reason.surfaces(), [SurfaceId::new(7)]);
    }

    #[test]
    fn a_global_reason_names_no_surface_at_all() {
        assert!(WakeReason::DeviceLost.surfaces().is_empty());
        assert!(WakeReason::ColorSchemeChanged.surfaces().is_empty());
    }

    #[test]
    fn a_tree_request_names_exactly_its_own_surface() {
        let reason = WakeReason::A11yTreeRequested(SurfaceId::new(3));
        assert_eq!(reason.surfaces(), [SurfaceId::new(3)]);
    }
}
