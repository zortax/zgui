//! Content dropped onto the window from outside it.

use std::path::PathBuf;

use zgui_geom::{Css, CssPx, Point};

/// What a drop event carries: everything dropped at once, and where.
///
/// A drop is one event carrying a set of paths rather than one event per path. That is not a
/// convenience: a handler told about three files one at a time cannot tell whether a fourth is
/// coming, so it either acts three times or waits for a signal that never arrives.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::DropEvent;
///
/// let event = DropEvent {
///     paths: vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
///     position: Point::new(CssPx(10.0), CssPx(20.0)),
/// };
/// assert_eq!(event.paths.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DropEvent {
    /// Everything dropped, in the order the platform listed it.
    pub paths: Vec<PathBuf>,
    /// Where the drop happened, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
}

impl DropEvent {
    /// Whether anything was actually dropped.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::DropEvent;
    use std::path::PathBuf;
    use zgui_geom::{CssPx, Point};

    #[test]
    fn a_drop_carries_every_path_at_once() {
        let event = DropEvent {
            paths: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            position: Point::new(CssPx(0.0), CssPx(0.0)),
        };
        assert!(!event.is_empty());
        assert_eq!(event.paths.len(), 2);
    }

    #[test]
    fn a_drop_of_nothing_is_expressible() {
        let event = DropEvent {
            paths: Vec::new(),
            position: Point::new(CssPx(0.0), CssPx(0.0)),
        };
        assert!(event.is_empty());
    }
}
