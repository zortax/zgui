//! Counting primary text presses without changing generic click activation.
use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::Timestamp;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::window) struct TextClicks {
    last: Option<(NodeKey, Timestamp, Point<CssPx, Css>, u8)>,
}
impl TextClicks {
    pub(in crate::window) fn note(
        &mut self,
        node: NodeKey,
        when: Timestamp,
        at: Point<CssPx, Css>,
    ) -> u8 {
        let count = self
            .last
            .filter(|(held, then, there, _)| {
                *held == node
                    && when.saturating_since(*then) <= crate::window::clicks::INTERVAL
                    && (at.x.0 - there.x.0).abs() <= crate::window::clicks::SLOP
                    && (at.y.0 - there.y.0).abs() <= crate::window::clicks::SLOP
            })
            .map_or(1, |(_, _, _, count)| if count == 3 { 1 } else { count + 1 });
        self.last = Some((node, when, at, count));
        count
    }
    pub(in crate::window) fn forget(&mut self) {
        self.last = None;
    }
}
