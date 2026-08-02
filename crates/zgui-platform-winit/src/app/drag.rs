//! Content dragged over a window from outside the application.

use std::path::PathBuf;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::DragEvent;

/// A drag in flight over one window.
///
/// The platform reports dragged files **one at a time**, with no event that says the set is
/// complete. The contract carries the whole set at every stage, and that difference is not
/// cosmetic: a drop target told about one file at a time cannot know whether more are coming, so it
/// either acts once per file or waits for an end that never arrives.
///
/// So the paths are gathered as they arrive and handed over as one event when the turn of the loop
/// that delivered them is finished. That is the earliest moment at which the set is known to be
/// complete, because the platform delivers a drag's files within a single turn.
#[derive(Debug, Default)]
pub(crate) struct Drag {
    /// Paths reported as hovering and not yet announced.
    hovering: Vec<PathBuf>,
    /// Paths reported as dropped and not yet announced.
    dropped: Vec<PathBuf>,
    /// Whether the drag left without being dropped.
    left: bool,
}

impl Drag {
    /// Records a path being dragged over the window.
    pub(crate) fn hovering(&mut self, path: PathBuf) {
        self.hovering.push(path);
    }

    /// Records a path being dropped on the window.
    pub(crate) fn dropped(&mut self, path: PathBuf) {
        self.dropped.push(path);
    }

    /// Records the drag leaving without a drop.
    pub(crate) const fn left(&mut self) {
        self.left = true;
    }

    /// Whether anything is waiting to be announced.
    pub(crate) const fn is_pending(&self) -> bool {
        self.left || !self.hovering.is_empty() || !self.dropped.is_empty()
    }

    /// Takes everything gathered, as the events a drop target is written against.
    ///
    /// A drop is announced after an entry, in that order, because a target that is told about a
    /// drop it was never told to expect has had no chance to say whether it would accept one.
    pub(crate) fn take(&mut self, position: Point<CssPx, Css>) -> Vec<DragEvent> {
        let mut events = Vec::new();
        if !self.hovering.is_empty() {
            events.push(DragEvent::Entered {
                paths: core::mem::take(&mut self.hovering),
                position,
            });
        }
        if !self.dropped.is_empty() {
            events.push(DragEvent::Dropped {
                paths: core::mem::take(&mut self.dropped),
                position,
            });
        }
        if core::mem::take(&mut self.left) {
            events.push(DragEvent::Left);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::Drag;
    use std::path::PathBuf;
    use zgui_geom::{Css, CssPx, Point};
    use zgui_platform::DragEvent;

    fn at(x: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(0.0))
    }

    #[test]
    fn three_files_dropped_at_once_arrive_as_one_event() {
        // Told about them one at a time, a target cannot know when the set is complete, so it
        // either acts three times or waits for an end that never comes.
        let mut drag = Drag::default();
        for name in ["/a.png", "/b.png", "/c.png"] {
            drag.dropped(PathBuf::from(name));
        }
        let events = drag.take(at(4.0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].paths().len(), 3);
        assert!(events[0].is_drop());
    }

    #[test]
    fn a_target_is_told_what_is_coming_before_it_is_told_it_arrived() {
        let mut drag = Drag::default();
        drag.hovering(PathBuf::from("/a.png"));
        drag.dropped(PathBuf::from("/a.png"));
        let events = drag.take(at(1.0));
        assert!(matches!(events[0], DragEvent::Entered { .. }));
        assert!(events[1].is_drop());
    }

    #[test]
    fn nothing_gathered_announces_nothing() {
        let mut drag = Drag::default();
        assert!(!drag.is_pending());
        assert!(drag.take(at(0.0)).is_empty());
    }

    #[test]
    fn a_drag_that_left_without_dropping_says_so_and_carries_no_paths() {
        let mut drag = Drag::default();
        drag.hovering(PathBuf::from("/a.png"));
        let _ = drag.take(at(0.0));
        drag.left();
        assert!(drag.is_pending());
        let events = drag.take(at(0.0));
        assert_eq!(events, vec![DragEvent::Left]);
        assert!(!drag.is_pending(), "the departure was announced twice");
    }
}
