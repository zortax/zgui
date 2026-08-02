//! Dragging something from one part of a window to another.
//!
//! This is **internal** drag and drop: within one window, between elements of one document. It is
//! entirely ours, and it works everywhere, because nothing outside the process is involved.
//!
//! Dragging *out* of the application is a different question with a different answer: the windowing
//! backend this framework ships against offers no outbound drag and no media-type negotiation at
//! all, so a control written against "dragging works" is written against something the desktop does
//! not provide. [`outbound_allowed`] is how a component asks before offering the affordance, and it
//! reads the platform's own declaration rather than guessing from the operating system's name.

pub mod image;
pub mod source;
pub mod target;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::PlatformCapabilities;

pub use crate::drag::image::DragImage;
pub use crate::drag::source::{DragSource, THRESHOLD};
pub use crate::drag::target::DropEffect;

/// What is happening to a drag right now.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum DragPhase {
    /// A press is being watched to see whether it becomes a drag.
    Armed,
    /// It has passed the threshold and is dragging.
    Dragging,
}

/// One drag in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    /// What is being dragged.
    pub source: DragSource,
    /// How far it is.
    pub phase: DragPhase,
    /// Where the press started.
    pub from: Point<CssPx, Css>,
    /// Where the pointer is now.
    pub at: Point<CssPx, Css>,
    /// What is under the pointer and willing to take it.
    pub over: Option<NodeKey>,
    /// What dropping on that would do.
    pub effect: DropEffect,
}

impl Drag {
    /// How far the pointer has moved since the press.
    pub fn offset(&self) -> Size<CssPx, Css> {
        Size::new(
            CssPx(self.at.x.0 - self.from.x.0),
            CssPx(self.at.y.0 - self.from.y.0),
        )
    }
}

/// What one drop produced.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Dropped {
    /// It landed on a target.
    On {
        /// What was dragged.
        source: DragSource,
        /// What it landed on.
        target: NodeKey,
        /// What that means.
        effect: DropEffect,
    },
    /// It was let go over nothing, or over something that refused it.
    Nowhere {
        /// What was dragged.
        source: DragSource,
    },
}

/// Whether a drag may leave this application for another one.
///
/// The honest answer on the backend this framework ships is *no*, and a component asks so that it
/// can offer a within-window drag — which does work — rather than an affordance that silently never
/// completes.
///
/// ```
/// use zgui_input::drag::outbound_allowed;
/// use zgui_platform::PlatformCapabilities;
///
/// assert!(!outbound_allowed(&PlatformCapabilities::none()));
/// ```
pub fn outbound_allowed(capabilities: &PlatformCapabilities) -> bool {
    capabilities.drag_source
}

/// The one drag a window can have in progress, and the rules for it.
///
/// A drag is armed by a press and only becomes a drag once the pointer has travelled: a press that
/// never moves is a click, and a draggable control that started dragging on press could never also
/// be clicked.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_input::drag::{Drags, DragPhase, DragSource};
/// use zgui_dom::{Document, EverythingMatters};
/// use zgui_interned::ElementName;
///
/// let document = Document::new();
/// let node = document
///     .edit(&EverythingMatters, |edit| {
///         let root = edit.create_element(ElementName::new("root"));
///         edit.insert_before(document.document_index(), root, None);
///         root
///     })
///     .expect("not poisoned");
/// let key = document.store().key_of(node);
///
/// let mut drags = Drags::default();
/// drags.arm(DragSource::node(key), Point::new(CssPx(0.0), CssPx(0.0)));
/// assert_eq!(drags.phase(), Some(DragPhase::Armed));
///
/// drags.moved(Point::new(CssPx(0.0), CssPx(2.0)), None);
/// assert_eq!(drags.phase(), Some(DragPhase::Armed), "two pixels is a click that wobbled");
///
/// drags.moved(Point::new(CssPx(0.0), CssPx(20.0)), None);
/// assert_eq!(drags.phase(), Some(DragPhase::Dragging));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Drags {
    /// The drag in progress, if there is one.
    live: Option<Drag>,
}

impl Drags {
    /// Nothing being dragged.
    pub fn new() -> Self {
        Self::default()
    }

    /// The drag in progress.
    pub fn current(&self) -> Option<&Drag> {
        self.live.as_ref()
    }

    /// How far the drag in progress has got.
    pub fn phase(&self) -> Option<DragPhase> {
        self.live.as_ref().map(|drag| drag.phase)
    }

    /// Whether something is actually being dragged, as opposed to merely pressed.
    pub fn is_dragging(&self) -> bool {
        self.phase() == Some(DragPhase::Dragging)
    }

    /// Watches a press to see whether it becomes a drag.
    pub fn arm(&mut self, source: DragSource, from: Point<CssPx, Css>) {
        self.live = Some(Drag {
            source,
            phase: DragPhase::Armed,
            from,
            at: from,
            over: None,
            effect: DropEffect::None,
        });
    }

    /// Records that the pointer moved, over `target` if it is over anything that accepts drops.
    ///
    /// Returns true when this move is the one that started the drag, which is when a drag image is
    /// created and the source is told it has left.
    pub fn moved(&mut self, to: Point<CssPx, Css>, target: Option<(NodeKey, DropEffect)>) -> bool {
        let Some(drag) = self.live.as_mut() else {
            return false;
        };
        drag.at = to;
        let (over, effect) = match target {
            Some((node, effect)) => (Some(node), effect),
            None => (None, DropEffect::None),
        };
        // A target the drag cannot land on is not a target: recording it would light up the drop
        // affordance on something that refuses the drop.
        let accepted = effect != DropEffect::None;
        drag.over = accepted.then_some(over).flatten();
        drag.effect = if accepted { effect } else { DropEffect::None };
        if drag.phase == DragPhase::Dragging {
            return false;
        }
        if source::past_threshold(drag.from, to) {
            drag.phase = DragPhase::Dragging;
            return true;
        }
        false
    }

    /// Ends the drag by letting go, answering with what that produced.
    ///
    /// A press that never became a drag produces nothing at all, so that the ordinary click path
    /// is untouched by having armed one.
    pub fn release(&mut self) -> Option<Dropped> {
        let drag = self.live.take()?;
        if drag.phase != DragPhase::Dragging {
            return None;
        }
        Some(match (drag.over, drag.effect) {
            (Some(target), effect) if effect != DropEffect::None => Dropped::On {
                source: drag.source,
                target,
                effect,
            },
            _ => Dropped::Nowhere {
                source: drag.source,
            },
        })
    }

    /// Ends the drag without dropping, which is what Escape and a lost input focus both mean.
    pub fn cancel(&mut self) -> Option<DragSource> {
        self.live.take().map(|drag| drag.source)
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKey};
    use zgui_geom::{Css, CssPx, Point};
    use zgui_interned::ElementName;
    use zgui_platform::PlatformCapabilities;

    use super::{DragPhase, DragSource, Drags, DropEffect, Dropped, outbound_allowed};

    fn nodes() -> (Document, NodeKey, NodeKey) {
        let document = Document::new();
        let (row, bin) = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let row = edit.create_element(ElementName::new("box"));
                edit.insert_before(root, row, None);
                let bin = edit.create_element(ElementName::new("box"));
                edit.insert_before(root, bin, None);
                (row, bin)
            })
            .expect("not poisoned");
        let store = document.store();
        let (row, bin) = (store.key_of(row), store.key_of(bin));
        (document, row, bin)
    }

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn a_press_that_never_travelled_produces_no_drop_at_all() {
        let (_document, row, _bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        drags.moved(at(1.0, 1.0), None);
        assert_eq!(
            drags.release(),
            None,
            "a click on a draggable control is a click"
        );
        assert_eq!(drags.phase(), None);
    }

    #[test]
    fn a_drag_that_lands_on_a_target_says_so() {
        let (_document, row, bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        assert!(drags.moved(at(0.0, 30.0), Some((bin, DropEffect::Move))));
        assert!(drags.is_dragging());
        assert_eq!(
            drags.release(),
            Some(Dropped::On {
                source: DragSource::node(row),
                target: bin,
                effect: DropEffect::Move,
            })
        );
    }

    #[test]
    fn a_target_that_refuses_the_drop_is_not_hovered() {
        let (_document, row, bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        drags.moved(at(0.0, 30.0), Some((bin, DropEffect::None)));
        assert_eq!(
            drags.current().and_then(|drag| drag.over),
            None,
            "lighting up a target that will refuse the drop is worse than showing nothing"
        );
        assert_eq!(
            drags.release(),
            Some(Dropped::Nowhere {
                source: DragSource::node(row)
            })
        );
    }

    #[test]
    fn the_start_of_the_drag_is_reported_exactly_once() {
        let (_document, row, _bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        assert!(drags.moved(at(0.0, 30.0), None));
        assert!(
            !drags.moved(at(0.0, 60.0), None),
            "every further move would otherwise create another drag image"
        );
    }

    #[test]
    fn cancelling_ends_it_without_dropping_anything() {
        let (_document, row, bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        drags.moved(at(0.0, 30.0), Some((bin, DropEffect::Copy)));
        assert_eq!(drags.cancel(), Some(DragSource::node(row)));
        assert_eq!(drags.phase(), None);
        assert_eq!(drags.release(), None);
    }

    #[test]
    fn the_offset_is_from_the_press_and_not_from_the_last_move() {
        let (_document, row, _bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(10.0, 10.0));
        drags.moved(at(10.0, 40.0), None);
        drags.moved(at(10.0, 60.0), None);
        assert_eq!(
            drags.current().expect("dragging").offset().height,
            CssPx(50.0)
        );
    }

    #[test]
    fn no_backend_here_offers_an_outbound_drag() {
        let mut capabilities = PlatformCapabilities::none();
        assert!(!outbound_allowed(&capabilities));
        capabilities.drag_source = true;
        assert!(
            outbound_allowed(&capabilities),
            "and a backend that gains one is believed without any component changing"
        );
    }

    #[test]
    fn the_phases_are_distinguishable() {
        let (_document, row, _bin) = nodes();
        let mut drags = Drags::new();
        drags.arm(DragSource::node(row), at(0.0, 0.0));
        assert_eq!(drags.phase(), Some(DragPhase::Armed));
        assert!(!drags.is_dragging());
    }
}
