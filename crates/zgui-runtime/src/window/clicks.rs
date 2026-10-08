//! Counting clicks into double clicks.
//!
//! A pointer activation that lands on the element the last one landed on, soon enough and near
//! enough, is a double click. The window dispatches `double_click` after the second `click`, down
//! the same path. A third press starts a new count, so a triple press gives one double click.

use std::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::Timestamp;

/// How long the second press of a double click may follow the first.
pub(crate) const INTERVAL: Duration = Duration::from_millis(500);

/// How far, in CSS pixels on either axis, the second press may land from the first.
pub(crate) const SLOP: f32 = 4.0;

/// The last pointer activation that may start a double click.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClickCount {
    /// The element, when, and where.
    last: Option<(NodeKey, Timestamp, Point<CssPx, Css>)>,
}

impl ClickCount {
    /// Notes a pointer activation of `node` at `at` and `when`, and answers whether it completes
    /// a double click.
    pub(crate) fn note(&mut self, node: NodeKey, when: Timestamp, at: Point<CssPx, Css>) -> bool {
        let double = self.last.is_some_and(|(held, then, there)| {
            held == node
                && when.saturating_since(then) <= INTERVAL
                && (at.x.0 - there.x.0).abs() <= SLOP
                && (at.y.0 - there.y.0).abs() <= SLOP
        });
        self.last = if double { None } else { Some((node, when, at)) };
        double
    }

    /// Forgets the last activation, as a keyboard activation does.
    pub(crate) fn forget(&mut self) {
        self.last = None;
    }
}
