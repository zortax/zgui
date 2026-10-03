//! How a touchpad gesture moves a document: its stretch past an end, its latch and its momentum.
//!
//! Each case delivers wheel events at the platform seam, with the phases a touchpad reports, and
//! reads the offsets and displacements the scroller composes.

mod support;

use std::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::Runtime;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// The time between two touchpad events.
const BETWEEN: Duration = Duration::from_millis(8);

/// A list that scrolls down only: its rows are wider than the port, and its horizontal overflow
/// is hidden.
const LIST: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow-x: hidden; overflow-y: scroll }
.row { display: block; width: 800px; height: 20px; background-color: #202020 }
";

/// A port whose content fits inside it.
const FITS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: auto }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// A touchpad over one document, with the clock its events are stamped by.
struct Pad {
    harness: Harness<Runtime>,
    stamp: Duration,
}

impl Pad {
    /// A window with `rows` rows in one port, styled by `css`.
    fn new(css: &str, rows: usize) -> Self {
        Self::build(css, rows, false)
    }

    fn build(css: &str, rows: usize, tall: bool) -> Self {
        let mut harness = support::app_with_text_on(
            Headless::new(),
            css,
            move |cx: &mut zgui_view::BuildCx<'_>| {
                use zgui_view::{IntoView, View};
                let mut port = zgui_elements::column().class("port");
                for _ in 0..rows {
                    port = port.child(zgui_elements::column().class("row"));
                }
                let mut root = zgui_elements::column().class("root").child(port);
                if tall {
                    root = root.child(zgui_elements::column().class("tall"));
                }
                Box::new(root.into_view().build(cx))
            },
        );
        harness.settle(8);
        Self {
            harness,
            stamp: Duration::from_secs(1),
        }
    }

    /// Delivers one scroll of `(x, y)` CSS pixels in `phase`, over the middle of the port.
    fn send(&mut self, x: f32, y: f32, phase: ScrollPhase) {
        self.stamp += BETWEEN;
        self.harness.deliver_to_first(SurfaceEvent::Wheel {
            event: WheelEvent {
                id: zgui_vocab::PointerId::MOUSE,
                kind: zgui_vocab::PointerKind::Mouse,
                position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
                delta: ScrollDelta::Pixels(Size::new(CssPx(x), CssPx(y))),
                phase,
            },
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::from_origin(self.stamp),
        });
        self.harness.settle(2);
    }

    /// Fingers down, then each delta of `moves` in turn.
    fn drag(&mut self, moves: &[(f32, f32)]) {
        self.send(0.0, 0.0, ScrollPhase::Started);
        for (x, y) in moves {
            self.send(*x, *y, ScrollPhase::Moved);
        }
    }

    /// Fingers up.
    fn lift(&mut self) {
        self.send(0.0, 0.0, ScrollPhase::Ended);
    }

    fn window(&self) -> &zgui_runtime::Window {
        self.harness.app().windows().first().expect("a window")
    }

    /// Every scroll container, outermost first.
    fn containers(&self) -> Vec<NodeKey> {
        let window = self.window();
        let layout = window.layout().borrow();
        let mut found: Vec<NodeKey> = layout
            .keys()
            .into_iter()
            .filter(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .filter_map(|key| layout.node(key).source)
            .collect();
        found.dedup();
        found
    }

    /// The innermost scroll container, which is the port.
    fn port(&self) -> NodeKey {
        *self.containers().last().expect("the fixture has a port")
    }

    /// Where `container` is scrolled to, in device pixels.
    fn offset_of(&self, container: NodeKey) -> Point<zgui_geom::DevicePx, zgui_geom::Device> {
        self.window().scroll().borrow().offset_of(container)
    }

    /// How far `container` is drawn past its end, in device pixels.
    fn stretch_of(&self, container: NodeKey) -> Size<zgui_geom::DevicePx, zgui_geom::Device> {
        self.window().scroll().borrow().elastic_of(container)
    }

    /// Whether every edge is back at its end.
    fn settled(&self) -> bool {
        self.window().scroll().borrow().settled()
    }
}

#[test]
fn content_that_fits_its_port_never_moves_under_a_gesture() {
    let mut pad = Pad::new(FITS, 3);
    let port = pad.port();
    pad.drag(&[(0.0, -30.0), (0.0, -30.0), (0.0, 40.0), (20.0, 0.0)]);
    assert_eq!(
        pad.stretch_of(port),
        Size::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0))
    );
    assert!(
        pad.settled(),
        "a port with nothing to scroll was pulled past its end"
    );
    pad.lift();
}

#[test]
fn a_port_that_scrolls_down_only_never_moves_sideways() {
    let mut pad = Pad::new(LIST, 50);
    let port = pad.port();
    pad.drag(&[(-30.0, -30.0), (-30.0, -30.0), (40.0, 0.0)]);
    assert_eq!(
        pad.stretch_of(port).width.0,
        0.0,
        "a port with a hidden horizontal overflow was pulled sideways"
    );
    assert_eq!(
        pad.offset_of(port).x.0,
        0.0,
        "a person scrolled a hidden axis"
    );
    pad.lift();
}
