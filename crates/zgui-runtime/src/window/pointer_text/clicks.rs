//! Counting presses into text into double and triple presses.
//!
//! A primary press that lands in the element the last one landed in, soon enough and near enough,
//! adds one to the count. The fourth press starts a new count. Activation keeps its own count in
//! [`crate::window::clicks`], because it counts releases and stops at two.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::Timestamp;

use crate::window::clicks::follows;

/// The last press into text, and how many presses its run holds.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::window) struct TextClicks {
    /// The element, when, where, and the count.
    last: Option<(NodeKey, Timestamp, Point<CssPx, Css>, u8)>,
}

impl TextClicks {
    /// Notes a press into `node` at `at` and `when`, and answers its count: 1, 2 or 3.
    pub(in crate::window) fn note(
        &mut self,
        node: NodeKey,
        when: Timestamp,
        at: Point<CssPx, Css>,
    ) -> u8 {
        let count = self
            .last
            .filter(|&(held, then, there, _)| follows((held, then, there), (node, when, at)))
            .map_or(1, |(.., count)| if count == 3 { 1 } else { count + 1 });
        self.last = Some((node, when, at, count));
        count
    }

    /// Forgets the last press, so that the next one counts as 1.
    pub(in crate::window) fn forget(&mut self) {
        self.last = None;
    }
}
