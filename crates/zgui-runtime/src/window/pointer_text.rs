//! Placing the caret with a pointer, and dragging a selection out of the text.
//!
//! This is a *framework default* in the same sense typing is: it runs after every listener on the
//! path, and only if none of them took responsibility for the event. A component that implements
//! its own text interaction — a code editor with rectangular selection, a field that selects a whole
//! token on click — cancels the press and gets the whole gesture to itself.
//!
//! One press places the caret, two presses select the run of word characters, punctuation or
//! space under the pointer, and three presses select the paragraph. A fourth press starts again
//! with a caret. A drag after the press extends the selection by the same unit. Only primary
//! presses select.
//!
//! The offset a press lands on is asked of the layout the frame produced, never of the string: the
//! bytes of a line are drawn in the order shaping decided, which on a line holding two directions
//! is not the order they are written in.

use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point};
use zgui_platform::SurfaceEvent;
use zgui_vocab::{PointerAction, PointerButton, PointerEvent, Timestamp};

use crate::window::Window;

mod clicks;
pub(super) use clicks::TextClicks;

/// The unit a press selects, and a drag after it extends the selection by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    /// One offset.
    Caret,
    /// One run of word characters, punctuation or space.
    Word,
    /// One paragraph.
    Paragraph,
}

/// A drag that is selecting text: the element it started in, and the offset it started at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selecting {
    /// The editable element.
    pub node: NodeKey,
    /// The offset the press landed on, which the selection is anchored to for the whole drag.
    pub anchor: usize,
    /// The other end of the run or paragraph the press selected.
    end: usize,
    /// What the press selected, which the drag extends by.
    unit: Unit,
}

impl Window {
    /// Places or extends the caret from a pointer event, and reports whether it did.
    ///
    /// A press inside an editable element places the caret and anchors a drag; every move while
    /// that drag is live extends the selection to wherever the pointer is now; the release ends it.
    /// A move with no drag live does nothing at all, which is what keeps a pointer merely crossing
    /// a field from selecting anything.
    pub(crate) fn point_at_text(&mut self, event: &SurfaceEvent, target: Option<NodeKey>) -> bool {
        let SurfaceEvent::Pointer {
            action,
            event: pointer,
            timestamp,
            ..
        } = event
        else {
            return false;
        };
        match action {
            PointerAction::Pressed => {
                // A press puts the caret somewhere of its own, which ends any run of vertical
                // arrow motions and the column it was aiming for.
                self.vertical_goal = None;
                self.press_into_text(pointer, target, *timestamp)
            }
            PointerAction::Moved => self.drag_through_text(pointer),
            PointerAction::Released | PointerAction::Cancelled => {
                if *action == PointerAction::Cancelled {
                    self.text_clicks.forget();
                }
                // A drag that ended is not a caret that moved, so this reports nothing: reporting
                // it would take the release away from every other default, including the
                // activation a click on a button is.
                self.selecting = None;
                false
            }
            _ => false,
        }
    }

    /// Places the caret where a press landed, and anchors a drag there.
    fn press_into_text(
        &mut self,
        pointer: &PointerEvent,
        target: Option<NodeKey>,
        timestamp: Timestamp,
    ) -> bool {
        // A press with no button is a primary press. A secondary or middle press selects nothing.
        if pointer
            .button
            .is_some_and(|button| button != PointerButton::Primary)
        {
            self.selecting = None;
            self.text_clicks.forget();
            return false;
        }
        let Some(node) = self.selectable_text_at(target) else {
            self.selecting = None;
            self.text_clicks.forget();
            return false;
        };
        // `user-select: none` on the field, or on anything above it that the field has not
        // overridden, means a press here is a press and not the start of a drag.
        if !self.selectable_at(node) {
            self.selecting = None;
            self.text_clicks.forget();
            return false;
        }
        let Some((offset, affinity)) = self.offset_at(node, self.device_point(pointer)) else {
            return false;
        };
        let count = self.text_clicks.note(node, timestamp, pointer.position);
        let unit = match count {
            2 => Unit::Word,
            3 => Unit::Paragraph,
            _ => Unit::Caret,
        };
        if unit == Unit::Caret {
            self.selecting = Some(Selecting {
                node,
                anchor: offset,
                end: offset,
                unit,
            });
            return self.write_caret(node, offset, offset, affinity);
        }
        // Placing the caret attaches the model, and the run or paragraph comes from its text.
        if !self.write_caret(node, offset, offset, affinity) {
            return false;
        }
        let range = self
            .selection_unit(node, offset, affinity, unit)
            .unwrap_or(offset..offset);
        self.selecting = Some(Selecting {
            node,
            anchor: range.start,
            end: range.end,
            unit,
        });
        // The selection ends at the end of its run or paragraph, before the next one.
        let focus_affinity = if range.is_empty() {
            affinity
        } else {
            zgui_edit::Affinity::Upstream
        };
        self.write_caret(node, range.start, range.end, focus_affinity)
    }

    /// Extends the live drag's selection to wherever the pointer is now.
    fn drag_through_text(&mut self, pointer: &PointerEvent) -> bool {
        let Some(drag) = self.selecting else {
            return false;
        };
        let Some((offset, affinity)) = self.offset_at(drag.node, self.device_point(pointer)) else {
            return false;
        };
        if drag.unit == Unit::Caret {
            if offset != drag.anchor {
                self.text_clicks.forget();
            }
            return self.write_caret(drag.node, drag.anchor, offset, affinity);
        }
        let Some(range) = self.selection_unit(drag.node, offset, affinity, drag.unit) else {
            return false;
        };
        if range.start != drag.anchor || range.end != drag.end {
            self.text_clicks.forget();
        }
        let (anchor, focus) = if range.end <= drag.anchor {
            (drag.end, range.start)
        } else {
            (drag.anchor, range.end.max(drag.end))
        };
        let focus_affinity = if focus < anchor {
            zgui_edit::Affinity::Downstream
        } else if focus > anchor {
            zgui_edit::Affinity::Upstream
        } else {
            affinity
        };
        self.write_caret(drag.node, anchor, focus, focus_affinity)
    }

    /// The run or paragraph of the model's text that holds a pointer's offset.
    fn selection_unit(
        &self,
        node: NodeKey,
        offset: usize,
        affinity: zgui_edit::Affinity,
        unit: Unit,
    ) -> Option<core::ops::Range<usize>> {
        match unit {
            Unit::Caret => Some(offset..offset),
            Unit::Word => self.editors.word_at(node, offset, affinity),
            Unit::Paragraph => self.editors.paragraph_at(node, offset),
        }
    }

    /// Writes one selection into the model and into the record, and restarts the blink.
    ///
    /// Both, always. The model is what the next keystroke replaces text at; the record is what a
    /// view reads and what the accessibility tree reports. A caret written into one of them only is
    /// a field that types somewhere other than where it is drawn.
    ///
    /// The affinity is the pointer's own answer and is carried rather than defaulted: an offset at
    /// a soft line break or a direction boundary names two places, and the one a person clicked is
    /// the one this knows.
    fn write_caret(
        &mut self,
        node: NodeKey,
        anchor: usize,
        focus: usize,
        affinity: zgui_edit::Affinity,
    ) -> bool {
        let edited = {
            let document = self.document.borrow();
            self.editors.place(&document, node, anchor, focus, affinity)
        };
        if !edited.handled {
            return false;
        }
        // The model's own selection rather than the response's: a press that landed where the caret
        // already was moves nothing and reports no change, and a record written only on a change
        // would then disagree with the model about a field that was clicked twice in the same place.
        let Some(selection) = self.editors.selection(node) else {
            return false;
        };
        self.host.write_selection(node, selection.range());
        self.carets.restart(self.clock.now());
        self.report_caret();
        edited.handled
    }

    /// The nearest element with selectable text at or above a node, if there is one.
    ///
    /// At or above, because a press lands on the text node's box or on an inline span inside the
    /// field rather than on the field itself, and a caret placed only when the field was hit
    /// exactly is a caret that never moves for a click on a letter.
    pub(crate) fn selectable_text_at(&self, node: Option<NodeKey>) -> Option<NodeKey> {
        let document = self.document.borrow();
        let mut index = document.store().index_of(node?)?;
        loop {
            let key = document.store().key_of(index);
            if crate::editing::Editors::is_selectable(&document, key) {
                return Some(key);
            }
            index = document.store().core(index).parent()?;
        }
    }

    /// Where a pointer is, in the absolute device pixels the fragment tree is measured in.
    fn device_point(&self, pointer: &PointerEvent) -> Point<DevicePx, Device> {
        Point::new(
            DevicePx(pointer.position.x.0 * self.scale),
            DevicePx(pointer.position.y.0 * self.scale),
        )
    }
}
