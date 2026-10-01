//! Which clicks make a double click.
//!
//! A double click is a second primary click on the same element, near the first one and soon after
//! it. The second click completes the pair, and the next click starts a new pair.

use core::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::Timestamp;

/// The longest time between the two clicks of a double click.
///
/// The default of Windows, GTK and macOS.
pub const INTERVAL: Duration = Duration::from_millis(500);

/// The largest distance, in CSS pixels, between the two clicks of a double click.
pub const DISTANCE: f32 = 5.0;

/// The clicks that can still become a double click.
#[derive(Clone, Copy, Debug, Default)]
pub struct Clicks {
    /// The first click of a pair, when one is waiting for its second.
    first: Option<Click>,
}

/// One click, as a double click reads it.
#[derive(Clone, Copy, Debug)]
struct Click {
    /// The element that was clicked.
    node: NodeKey,
    /// Where the pointer was.
    at: Point<CssPx, Css>,
    /// When the pointer was released.
    when: Timestamp,
}

impl Clicks {
    /// Nothing clicked yet.
    pub const fn new() -> Self {
        Self { first: None }
    }

    /// Records a primary click on `node`, and tells whether it completes a double click.
    pub fn click(&mut self, node: NodeKey, at: Point<CssPx, Css>, when: Timestamp) -> bool {
        let second = Click { node, at, when };
        let paired = self.first.take().is_some_and(|first| pairs(first, second));
        if !paired {
            self.first = Some(second);
        }
        paired
    }

    /// Forgets the waiting click, so that the next click starts a new pair.
    pub fn reset(&mut self) {
        self.first = None;
    }
}

/// Whether `second` completes a double click that `first` started.
fn pairs(first: Click, second: Click) -> bool {
    let dx = second.at.x.0 - first.at.x.0;
    let dy = second.at.y.0 - first.at.y.0;
    first.node == second.node
        && second.when.saturating_since(first.when) <= INTERVAL
        && dx * dx + dy * dy <= DISTANCE * DISTANCE
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_dom::{Document, EverythingMatters, NodeKey};
    use zgui_geom::{Css, CssPx, Point};
    use zgui_interned::ElementName;
    use zgui_vocab::Timestamp;

    use super::{Clicks, DISTANCE, INTERVAL};

    fn two_nodes() -> (NodeKey, NodeKey) {
        let document = Document::new();
        document
            .edit(&EverythingMatters, |edit| {
                let a = edit.create_element(ElementName::new("control"));
                let b = edit.create_element(ElementName::new("control"));
                (document.store().key_of(a), document.store().key_of(b))
            })
            .expect("not poisoned")
    }

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    fn after(millis: u64) -> Timestamp {
        Timestamp::from_origin(Duration::from_millis(millis))
    }

    #[test]
    fn two_quick_clicks_on_one_element_are_a_double_click() {
        let (node, _) = two_nodes();
        let mut clicks = Clicks::new();
        assert!(!clicks.click(node, at(10.0, 10.0), after(0)));
        assert!(clicks.click(node, at(12.0, 11.0), after(200)));
    }

    #[test]
    fn a_third_click_starts_a_new_pair() {
        let (node, _) = two_nodes();
        let mut clicks = Clicks::new();
        assert!(!clicks.click(node, at(0.0, 0.0), after(0)));
        assert!(clicks.click(node, at(0.0, 0.0), after(100)));
        assert!(!clicks.click(node, at(0.0, 0.0), after(200)));
        assert!(clicks.click(node, at(0.0, 0.0), after(300)));
    }

    #[test]
    fn a_slow_second_click_starts_a_new_pair() {
        let (node, _) = two_nodes();
        let mut clicks = Clicks::new();
        let late = INTERVAL.as_millis() as u64 + 1;
        assert!(!clicks.click(node, at(0.0, 0.0), after(0)));
        assert!(!clicks.click(node, at(0.0, 0.0), after(late)));
        assert!(clicks.click(node, at(0.0, 0.0), after(late + 100)));
    }

    #[test]
    fn a_distant_second_click_is_no_double_click() {
        let (node, _) = two_nodes();
        let mut clicks = Clicks::new();
        assert!(!clicks.click(node, at(0.0, 0.0), after(0)));
        assert!(!clicks.click(node, at(DISTANCE + 1.0, 0.0), after(100)));
    }

    #[test]
    fn a_second_click_on_another_element_is_no_double_click() {
        let (first, second) = two_nodes();
        let mut clicks = Clicks::new();
        assert!(!clicks.click(first, at(0.0, 0.0), after(0)));
        assert!(!clicks.click(second, at(0.0, 0.0), after(100)));
    }

    #[test]
    fn a_reset_forgets_the_first_click() {
        let (node, _) = two_nodes();
        let mut clicks = Clicks::new();
        assert!(!clicks.click(node, at(0.0, 0.0), after(0)));
        clicks.reset();
        assert!(!clicks.click(node, at(0.0, 0.0), after(100)));
    }
}
