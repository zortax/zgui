//! Content being dragged over a surface from outside it.

use std::path::PathBuf;

use zgui_geom::{Css, CssPx, Point};

/// A stage of a drag that started outside the application.
///
/// The whole set of paths arrives at once, with a position, at every stage. That shape is not a
/// convenience: told about one file at a time, a drop target cannot know whether more are coming,
/// so it either acts once per file or waits for an end that never comes. It also lets a target
/// decide whether it *would* accept the drop while the pointer is still moving, which is what
/// makes a highlight appear before the user lets go rather than after.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_geom::{CssPx, Point};
/// use zgui_platform::DragEvent;
///
/// let entered = DragEvent::Entered {
///     paths: vec![PathBuf::from("/tmp/a.png")],
///     position: Point::new(CssPx(4.0), CssPx(4.0)),
/// };
/// assert_eq!(entered.paths().len(), 1);
/// assert!(!entered.is_drop());
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DragEvent {
    /// A drag came over the surface, carrying these paths.
    Entered {
        /// Everything being dragged.
        paths: Vec<PathBuf>,
        /// Where the pointer is, in CSS pixels from the surface's top-left corner.
        position: Point<CssPx, Css>,
    },
    /// The drag moved while over the surface.
    Moved {
        /// Where the pointer is now.
        position: Point<CssPx, Css>,
    },
    /// The drag was released over the surface.
    Dropped {
        /// Everything dropped.
        paths: Vec<PathBuf>,
        /// Where it was dropped.
        position: Point<CssPx, Css>,
    },
    /// The drag left the surface without being dropped.
    Left,
}

impl DragEvent {
    /// Everything being dragged, which is nothing on the stages that do not carry it.
    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Self::Entered { paths, .. } | Self::Dropped { paths, .. } => paths,
            Self::Moved { .. } | Self::Left => &[],
        }
    }

    /// Where the pointer is, when this stage says.
    pub fn position(&self) -> Option<Point<CssPx, Css>> {
        match self {
            Self::Entered { position, .. }
            | Self::Moved { position }
            | Self::Dropped { position, .. } => Some(*position),
            Self::Left => None,
        }
    }

    /// Whether this stage is the release.
    pub const fn is_drop(&self) -> bool {
        matches!(self, Self::Dropped { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::DragEvent;
    use std::path::PathBuf;
    use zgui_geom::{Css, CssPx, Point};

    fn at(x: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(0.0))
    }

    #[test]
    fn only_the_carrying_stages_report_paths() {
        let entered = DragEvent::Entered {
            paths: vec![PathBuf::from("/a")],
            position: at(1.0),
        };
        assert_eq!(entered.paths().len(), 1);
        assert!(DragEvent::Moved { position: at(2.0) }.paths().is_empty());
        assert!(DragEvent::Left.paths().is_empty());
    }

    #[test]
    fn leaving_reports_no_position_and_every_other_stage_does() {
        assert_eq!(DragEvent::Left.position(), None);
        assert_eq!(
            DragEvent::Moved { position: at(3.0) }.position(),
            Some(at(3.0))
        );
    }

    #[test]
    fn exactly_one_stage_is_the_release() {
        let dropped = DragEvent::Dropped {
            paths: Vec::new(),
            position: at(0.0),
        };
        assert!(dropped.is_drop());
        assert!(!DragEvent::Left.is_drop());
    }
}
