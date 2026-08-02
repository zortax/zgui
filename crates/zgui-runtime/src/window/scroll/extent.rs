//! Keeping the reader's position honest when the window changes under them.
//!
//! A scroll offset is written once and then stands, and everything it means depends on two numbers
//! it does not own: how tall the scrollport is and how far the content reaches inside it. A window
//! that is resized moves the first, a document that reflows moves the second, and a surface that
//! changes its device pixel ratio changes the unit both are measured in — none of which is a scroll,
//! so nothing about them writes an offset.
//!
//! The rule the two calls here enforce is stated in full in
//! [`zgui_scroll::scroller::extent`](zgui_scroll::scroller): a container that has become scrolled
//! past its end is clamped back to it, a container that is still inside its content is left exactly
//! where it is, and a change of ratio moves every offset by the change so that the same place in the
//! document keeps the same position on the screen.
//!
//! # Why they are marked and no frame is asked for
//!
//! Both run inside a frame, ahead of the stage that reads what they wrote. The clamp runs between
//! the layout pass whose extents it clamps against and the fragment pass that composes against the
//! result; the rescale runs when the configure is taken, which is before either. Marking the
//! container is what makes the fragment pass descend through it — that is the whole of what a
//! scroll owes — and asking for a *frame* on top of that would buy a second one that recomposes an
//! unchanged document and presents a surface identical to the one this frame is about to present.

use crate::window::Window;

impl Window {
    /// Brings every container back inside what its content now allows.
    ///
    /// Called after a layout pass that actually ran, which is the only kind that can have moved an
    /// extent. Costs one region lookup per container that has ever been scrolled, and marks nothing
    /// at all in the overwhelmingly common case where every offset is still legal — a window that
    /// grew, a window that changed only its width, and every relayout that was not a resize.
    pub(crate) fn clamp_scroll_to_content(&mut self) {
        let clamped = {
            let layout = self.layout.borrow();
            self.scroll.borrow_mut().reclamp(&layout)
        };
        if clamped.is_empty() {
            return;
        }
        zgui_profile::latency::note_with("w.reclamp", || clamped.len().to_string());
        let mut document = self.document.borrow_mut();
        zgui_scroll::mark::scrolled(&mut document, &clamped);
    }

    /// Carries every offset across a change of device pixel ratio.
    ///
    /// `by` is the new ratio over the old one. Nothing is clamped here: the extent the new offsets
    /// belong beside is the one the layout pass at the new ratio will produce, and
    /// [`Window::clamp_scroll_to_content`] after that pass is what bounds them.
    pub(crate) fn rescale_scroll(&mut self, by: f32) {
        let moved = self.scroll.borrow_mut().rescale(by);
        if moved.is_empty() {
            return;
        }
        let mut document = self.document.borrow_mut();
        zgui_scroll::mark::scrolled(&mut document, &moved);
    }
}
