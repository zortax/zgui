//! The scrollbars pointers are holding.
//!
//! A scrollbar is the one piece of a document whose behaviour spans several events and belongs to
//! no element's handler. A press on a thumb is not an action; it is the beginning of one, and what
//! makes the rest of it right is a single number carried from the press to every move that follows
//! — how far down the thumb the pointer went down. Losing it is the difference between dragging a
//! scrollbar and throwing it: the thumb jumps its edge to the pointer, the content jumps with it,
//! and the drag then works perfectly from the wrong place.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_layout::Axis;
use zgui_vocab::PointerId;

use crate::dispatch::FrameworkDefault;
use crate::hit::ScrollbarPress;

/// One scrollbar a pointer is holding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Held {
    /// The pointer holding it.
    pub(crate) pointer: PointerId,
    /// The element that scrolls.
    pub(crate) container: NodeKey,
    /// Which of its bars.
    pub(crate) axis: Axis,
    /// How far the pointer was from the thumb's near edge when it went down.
    ///
    /// Nothing for a press on the track: a track is pressed once and nothing about it follows the
    /// pointer afterwards, which is what stops a press that lands beside the thumb and then drifts
    /// from paging the document repeatedly.
    pub(crate) grab: Option<f32>,
}

/// What a pointer event turned out to be about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Bar {
    /// Not a scrollbar at all, so the ordinary press, focus and activation behaviour applies.
    Untouched,
    /// A scrollbar took the event, and this is what it asks for.
    ///
    /// A press on a thumb asks for nothing: it begins a drag, and the drag is what moves anything.
    Took(Option<FrameworkDefault>),
}

/// Every scrollbar currently held, at most one per pointer.
#[derive(Debug, Default)]
pub(crate) struct Bars {
    /// The bars being held, in no particular order.
    held: SmallVec<[Held; 2]>,
}

impl Bars {
    /// What one pointer is holding, if it is holding anything.
    pub(crate) fn of(&self, pointer: PointerId) -> Option<Held> {
        self.held
            .iter()
            .find(|held| held.pointer == pointer)
            .copied()
    }

    /// Records what a press on `press` began, and answers what the press itself asks for.
    pub(crate) fn press(
        &mut self,
        pointer: PointerId,
        press: &ScrollbarPress,
    ) -> Option<FrameworkDefault> {
        let grab = match press.part {
            zgui_layout::fragment::ScrollbarPart::Thumb => press.grab(),
            zgui_layout::fragment::ScrollbarPart::Track => None,
        };
        let held = Held {
            pointer,
            container: press.container,
            axis: press.axis,
            grab,
        };
        match self.held.iter_mut().find(|other| other.pointer == pointer) {
            Some(other) => *other = held,
            None => self.held.push(held),
        }
        match press.part {
            zgui_layout::fragment::ScrollbarPart::Thumb => None,
            zgui_layout::fragment::ScrollbarPart::Track => {
                press
                    .pages_forward()
                    .map(|forward| FrameworkDefault::ScrollPage {
                        container: press.container,
                        axis: press.axis,
                        forward,
                    })
            }
        }
    }

    /// Lets go of whatever one pointer was holding.
    pub(crate) fn release(&mut self, pointer: PointerId) {
        self.held.retain(|held| held.pointer != pointer);
    }

    /// Lets go of everything, which is what a cancelled interaction leaves owing.
    pub(crate) fn clear(&mut self) {
        self.held.clear();
    }

    /// Forgets every bar belonging to an element that has gone.
    pub(crate) fn forget(&mut self, node: NodeKey) {
        self.held.retain(|held| held.container != node);
    }
}

/// Where a drag has moved the container it is holding, given where the pointer is now.
///
/// Read from the fragment tree rather than from anything recorded at the press, so a drag survives
/// the container being resized under it: the thumb goes on meaning the same fraction of a track
/// that is now a different length.
pub(crate) fn dragged(
    layout: &zgui_layout::LayoutStore,
    held: &Held,
    at: f32,
) -> Option<FrameworkDefault> {
    let grab = held.grab?;
    let travel =
        zgui_layout::scroll_region::bar::live::travel_of(layout, held.container, held.axis)?;
    Some(FrameworkDefault::ScrollAlong {
        container: held.container,
        axis: held.axis,
        to: travel.offset_at(at - grab),
    })
}
