//! What a held offset does when the extent under it moves.
//!
//! An offset is written once and read for as long as nobody scrolls again, and everything it means
//! is a statement about numbers it does not own. How far a container *may* be scrolled is its
//! content's extent less its scrollport's, and both of those are layout's answers: a window that is
//! resized, a surface that changes its device pixel ratio, a panel that collapses and a list that
//! loses half its rows all move one or both without the offset being touched at all.
//!
//! So clamping at the moment of writing is not enough. It establishes that an offset was legal when
//! it was made, which is a claim about a document that no longer exists by the time the frame that
//! matters runs. The two entry points here are what re-establish it against the document that does.
//!
//! # The rule, stated for an application author
//!
//! **A resize never moves the reader on purpose, and never leaves them past the end.**
//!
//! * The window becomes **shorter**, or the content becomes **smaller**, so that the offset is now
//!   past the end: the offset is clamped down to the new end. The alternative is content composed
//!   off the top of the scrollport with blank below it — and at an offset far enough past the end,
//!   a window with nothing in it at all.
//! * The window becomes **taller**, or changes only its **width**, so that the offset is still
//!   legal: nothing moves. The line being read stays exactly where it is on the screen and only the
//!   amount of document around it changes. An offset is not re-derived as a fraction of the extent,
//!   because a fraction is not what a reader is looking at.
//! * The **device pixel ratio** changes: every offset is multiplied by the change. An offset is a
//!   number of *device* pixels, so the same position in the document is a different number on a
//!   surface with twice as many of them — and carrying the number across unchanged moves a reader
//!   who was at the bottom of a document to halfway down it, which is the same jump the clamp
//!   exists to prevent, in the other direction.
//!
//! Clamping is deliberately the *only* correction. A container that is legal where it stands is
//! left exactly where it stands, whatever else the resize did to the document around it.
//!
//! # Where each is answered
//!
//! [`Scroller::rescale`] is called by whoever is told the ratio moved, before the layout that
//! re-measures the document at the new one — the two are separate events to the offsets, and the
//! ratio is knowable without laying anything out. [`Scroller::reclamp`] is called after any layout
//! pass that actually ran, because the extent it clamps against is that pass's output. A pass that
//! was skipped produced the numbers already held, so nothing it could clamp against has moved and
//! the sweep is not worth making.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::region_of_element;

use crate::scroller::Scroller;

impl Scroller {
    /// Brings every held offset back inside what the content now allows, and reports what moved.
    ///
    /// A container whose offset is still legal is not touched and is not reported, which is what
    /// makes this callable after every layout pass: the common resize moves no offset at all, and
    /// the sweep costs one region lookup per container that has ever been scrolled.
    ///
    /// A container the store knows nothing about is skipped rather than sent to the origin. An
    /// element with no boxes is one whose subtree is between rebuilds or is not mounted at the
    /// moment; its offset is held under the element precisely so that it survives that, and
    /// clamping it against an extent that has not been measured would answer "zero" and lose the
    /// reader's position for exactly the reason offsets are not filed under boxes at all.
    ///
    /// Anything carrying the container towards somewhere past the new end is left running: a motion
    /// re-clamps against the store on every step it takes, so it converges on the new end by
    /// itself and stops there.
    pub fn reclamp(&mut self, store: &LayoutStore) -> SmallVec<[NodeKey; 2]> {
        let mut moved: SmallVec<[NodeKey; 2]> = SmallVec::new();
        let held: SmallVec<[(NodeKey, Point<DevicePx, Device>); 4]> = self.offsets.iter().collect();
        for (container, at) in held {
            let Some(region) = region_of_element(store, container) else {
                continue;
            };
            let limit = region.limit();
            let to = Point::new(
                DevicePx(at.x.0.clamp(0.0, limit.x.0)),
                DevicePx(at.y.0.clamp(0.0, limit.y.0)),
            );
            if to == at {
                continue;
            }
            self.offsets.place(container, to);
            self.compose(container);
            self.record(container, at, to);
            moved.push(container);
        }
        moved
    }

    /// Multiplies every offset by a change of device pixel ratio, and reports every container.
    ///
    /// `by` is the new ratio over the old one, so a window dragged from a one-times output to a
    /// two-times one passes `2.0`. Everything held here is in device pixels — the clamped offset,
    /// the composed one, and the elastic displacement and its speed — so all of them are the same
    /// position expressed in a unit that has just changed size.
    ///
    /// Nothing is clamped here, and that is not an omission. The extent the new offsets belong
    /// beside is the one a layout pass at the new ratio produces, which has not run yet; clamping
    /// against the old ratio's extent would cut the offset to a limit measured in the wrong unit
    /// and lose the position outright. [`Scroller::reclamp`] after that pass is what re-establishes
    /// the bound, and it is exact.
    ///
    /// Every container is reported rather than only those whose number changed, because at `by` of
    /// one this is not called at all: the caller only reaches it when the ratio moved, and at a
    /// ratio that moved a container at the origin is the one container whose composed position is
    /// genuinely unchanged and it is cheaper to mark it than to test for it.
    pub fn rescale(&mut self, by: f32) -> SmallVec<[NodeKey; 2]> {
        let mut moved: SmallVec<[NodeKey; 2]> = SmallVec::new();
        if !by.is_finite() || by <= 0.0 || by == 1.0 {
            return moved;
        }
        // The stretch first, and separately, because the two maps do not hold the same containers:
        // an edge dragged past the end of a list nobody has scrolled is displaced while its offset
        // is still the origin, and a pass driven by the offsets alone would leave that stretch
        // measured in the ratio it was made at.
        let stretched: SmallVec<[NodeKey; 2]> = self.elastic.keys().copied().collect();
        for container in stretched {
            let edge = self.overscroll_of(container).scaled(by);
            self.elastic.insert(container, edge);
            self.compose(container);
            moved.push(container);
        }
        let held: SmallVec<[(NodeKey, Point<DevicePx, Device>); 4]> = self.offsets.iter().collect();
        for (container, at) in held {
            let to = Point::new(DevicePx(at.x.0 * by), DevicePx(at.y.0 * by));
            self.offsets.place(container, to);
            self.compose(container);
            self.record(container, at, to);
            if !moved.contains(&container) {
                moved.push(container);
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::NodeKey;
    use zgui_geom::{DevicePx, Point};

    use crate::scroller::Scroller;

    fn key(index: u32) -> NodeKey {
        NodeKey::new(
            index,
            zgui_arena::Generation::FIRST,
            zgui_arena::DomainId::FIRST,
        )
    }

    #[test]
    fn a_ratio_that_doubled_doubles_the_number_that_stands_for_the_same_position() {
        // The reader is nine tenths of the way down a document. On a surface with twice as many
        // device pixels the same place is twice the number, and carrying the old number across
        // puts them just under halfway.
        let container = key(2);
        let mut scroller = Scroller::new();
        scroller
            .offsets
            .place(container, Point::new(DevicePx(0.0), DevicePx(5_670.0)));
        scroller.compose(container);

        let moved = scroller.rescale(2.0);
        assert_eq!(moved.as_slice(), &[container]);
        assert_eq!(scroller.offset_of(container).y, DevicePx(11_340.0));
        assert_eq!(scroller.composed().of(container).y, DevicePx(11_340.0));
    }

    #[test]
    fn a_ratio_that_did_not_move_touches_nothing() {
        let container = key(2);
        let mut scroller = Scroller::new();
        scroller
            .offsets
            .place(container, Point::new(DevicePx(0.0), DevicePx(120.0)));
        assert!(scroller.rescale(1.0).is_empty());
        assert!(scroller.rescale(f32::NAN).is_empty());
        assert!(scroller.rescale(0.0).is_empty());
        assert_eq!(scroller.offset_of(container).y, DevicePx(120.0));
    }

    #[test]
    fn a_displaced_edge_is_rescaled_with_the_offset_it_hangs_off() {
        let container = key(7);
        let mut scroller = Scroller::new();
        scroller.displace(
            container,
            crate::elastic::Overscroll::default()
                .pulled_by(zgui_geom::Size::new(DevicePx(0.0), DevicePx(60.0))),
        );
        let before = scroller.elastic_of(container).height.0;
        scroller.rescale(2.0);
        assert!(
            (scroller.elastic_of(container).height.0 - before * 2.0).abs() < 0.001,
            "the stretch was left measured in the previous ratio's pixels"
        );
    }
}
