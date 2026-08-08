//! Deciding whether this frame's scroll can be answered by moving pixels that are already drawn.
//!
//! A scroll moves every pixel of a scrollport, so it damages the whole port, so the emit walk
//! reaches every fragment in it and every primitive is pushed and re-ordered again — 77 % of a
//! scroll frame, measured in `docs/perf/scroll-frame.md`. None of that derives anything new. The
//! pixels are already in the renderer's composed target, one whole-pixel translation from where
//! they now belong.
//!
//! This is the gate. It says yes only for a frame that did one thing, and the fallback for every
//! other frame is the frame the window would have drawn anyway — so a refusal costs the saving and
//! never the picture.
//!
//! # The five questions
//!
//! 1. **Can the renderer do it at all?** A renderer without a persistent composed target cannot,
//!    and is never handed a damage set short of what it has to draw.
//! 2. **Did exactly one container move?** Two scrollers gliding at once are two translations, and
//!    one copy answers neither.
//! 3. **Did the walk service the frame incrementally, and did its moves agree?** Anything else the
//!    frame damaged is kept, in `RigidMoves::beyond`, and drawn on top of the moved pixels — which
//!    is what lets a scrollbar thumb move, or a row arrive at the edge, without costing the port.
//! 4. **Was the movement whole pixels, and settled?** A fraction cannot be copied, and an elastic
//!    bounce is deliberately fractional (`Scroller::compose`) so that it does not ratchet.
//! 5. **Is the port the container's alone, and opaque?** Asked of the document, in
//!    [`zgui_paint::shiftable`].

use zgui_geom::{Device, Rect};
use zgui_paint::shiftable;
use zgui_render::ScrollShift;

use crate::window::Window;

/// Why a frame's scroll was not answered by moving pixels.
///
/// Every arm is counted, because "scrolling is not being shifted" and "scrolling is not being
/// shifted *because this list has no background*" are different bugs and only one of them is the
/// framework's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The renderer keeps no composed target to move pixels within.
    NoComposedTarget,
    /// Nothing scrolled, or more than one thing did.
    NotOneContainer,
    /// The frame drew something the movement does not account for.
    MoreThanAMove,
    /// The displacement is not a whole number of device pixels, or the container is bouncing.
    NotWholePixels,
    /// The document refused it. Carries which of its reasons.
    Document(shiftable::Refusal),
}

impl Window {
    /// This frame's scroll as a movement of pixels, when it is one.
    ///
    /// `viewport` is the surface's extent, which the port is cut to: nothing outside the surface is
    /// in the composed target to be moved.
    pub(crate) fn scroll_shift(
        &self,
        viewport: zgui_geom::Size<i32, Device>,
    ) -> Result<ScrollShift, Refused> {
        if !self.renderer.shifts_composed_pixels() {
            return Err(Refused::NoComposedTarget);
        }
        let [scrolled] = self.scrolled_this_frame.as_slice() else {
            return Err(Refused::NotOneContainer);
        };
        let moves = self.rigid_moves;
        if !moves.settled || moves.conflicted || moves.count == 0 {
            return Err(Refused::MoreThanAMove);
        }

        // Taken from the walk's own report rather than from the two offsets, because the walk is
        // what actually moved the fragments and the two must not be able to disagree.
        let Some((dx, dy)) = moves.by else {
            return Err(Refused::MoreThanAMove);
        };
        if dx.fract() != 0.0 || dy.fract() != 0.0 {
            return Err(Refused::NotWholePixels);
        }
        // An elastic bounce is fractional on purpose and is not finished moving; a container in one
        // is composed against a displacement this frame's whole-pixel copy cannot express.
        if !self
            .scroll
            .borrow()
            .elastic_of(scrolled.container)
            .is_empty()
        {
            return Err(Refused::NotWholePixels);
        }

        let port = {
            let layout = self.layout.borrow();
            shiftable::port_may_be_shifted(&layout, scrolled.container)
                .map_err(Refused::Document)?
        };
        let surface = Rect::new(zgui_geom::Point::new(0, 0), viewport);
        let Some(port) = zgui_layout::fragment::diff::pixels(port).intersection(surface) else {
            return Err(Refused::NotOneContainer);
        };
        Ok(ScrollShift {
            port,
            by: (dx as i32, dy as i32),
        })
    }
}
