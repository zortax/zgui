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

/// One frame, rounded up past the deadline the park installs.
const FRAME: Duration = Duration::from_millis(20);

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

/// A short list inside a page that scrolls down.
const NESTED: &str = "
root { display: block; width: 400px; height: 300px; overflow-y: scroll }
.port { display: block; width: 400px; height: 120px; overflow-y: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.tall { display: block; width: 400px; height: 2000px }
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

    /// The same, with the port followed by a tall block in the root.
    fn nested(rows: usize) -> Self {
        Self::build(NESTED, rows, true)
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

    /// Runs `frames` frames of the clock with no input.
    fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.harness.advance(FRAME);
            self.harness.pump();
        }
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

#[test]
fn a_stretch_holds_under_the_fingers_and_returns_once_they_lift() {
    let mut pad = Pad::new(LIST, 50);
    let port = pad.port();
    pad.drag(&[(0.0, -40.0), (0.0, -40.0)]);
    let held = pad.stretch_of(port).height.0;
    assert!(held < 0.0, "the content did not follow past the top");

    // Fingers resting: no events, and the edge stays where the finger put it.
    pad.run(30);
    assert_eq!(
        pad.stretch_of(port).height.0,
        held,
        "the edge moved under a still finger"
    );
    assert!(
        !pad.window().scroll().borrow().is_animating(),
        "an edge that does not move asks for frames"
    );

    pad.lift();
    let mut seen = vec![held];
    for _ in 0..30 {
        pad.run(1);
        seen.push(pad.stretch_of(port).height.0);
    }
    assert!(
        seen.windows(2)
            .all(|pair| pair[1] >= pair[0] && pair[1] <= 0.0),
        "the return was not monotonic: {seen:?}"
    );
    assert!(pad.settled(), "the edge never came back: {seen:?}");
    assert_eq!(pad.offset_of(port).y.0, 0.0);
}

#[test]
fn moving_back_undoes_the_stretch_before_the_content_scrolls() {
    let mut pad = Pad::new(LIST, 50);
    let port = pad.port();
    pad.drag(&[(0.0, -100.0)]);
    let held = pad.stretch_of(port).height.0;

    pad.send(0.0, 40.0, ScrollPhase::Moved);
    let partly = pad.stretch_of(port).height.0;
    assert!(held < partly && partly < 0.0, "{held} then {partly}");
    assert_eq!(
        pad.offset_of(port).y.0,
        0.0,
        "the content scrolled while stretched"
    );

    pad.send(0.0, 100.0, ScrollPhase::Moved);
    assert_eq!(pad.stretch_of(port).height.0, 0.0);
    assert!(
        (pad.offset_of(port).y.0 - 40.0).abs() < 0.5,
        "the rest of the movement did not scroll the content: {}",
        pad.offset_of(port).y.0
    );
    pad.lift();
}

#[test]
fn a_gesture_stays_on_the_list_it_started_on() {
    let mut pad = Pad::nested(10);
    let [page, list] = pad.containers()[..] else {
        panic!("the fixture has a page and a list");
    };
    // The list scrolls 80 pixels, then reaches its end; the page has plenty of room.
    pad.drag(&[(0.0, 30.0); 6]);
    assert_eq!(pad.offset_of(list).y.0, 80.0);
    assert!(
        pad.stretch_of(list).height.0 > 0.0,
        "the list did not stretch at its end"
    );
    assert_eq!(
        pad.offset_of(page).y.0,
        0.0,
        "the page took over halfway through the gesture"
    );
    pad.lift();
    pad.run(40);

    // A new gesture over the list at its end goes to the page.
    pad.drag(&[(0.0, 30.0)]);
    assert_eq!(pad.offset_of(page).y.0, 30.0);
    pad.lift();
}

#[test]
fn a_gesture_no_container_can_follow_stretches_the_outermost_one() {
    let mut pad = Pad::nested(10);
    let [page, list] = pad.containers()[..] else {
        panic!("the fixture has a page and a list");
    };
    pad.drag(&[(0.0, -30.0), (0.0, -30.0)]);
    assert_eq!(pad.stretch_of(list).height.0, 0.0);
    assert!(
        pad.stretch_of(page).height.0 < 0.0,
        "the page did not stretch"
    );
    pad.lift();
}

#[test]
fn the_end_of_a_gesture_over_nothing_that_scrolls_still_lets_go() {
    let mut pad = Pad::new(LIST, 50);
    let port = pad.port();
    pad.drag(&[(0.0, -60.0)]);
    assert!(pad.stretch_of(port).height.0 < 0.0);

    // The pointer left the port before the fingers lifted.
    pad.stamp += BETWEEN;
    pad.harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(250.0)),
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(0.0))),
            phase: ScrollPhase::Ended,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(pad.stamp),
    });
    pad.harness.settle(2);
    pad.run(40);
    assert!(
        pad.settled(),
        "the edge stayed stretched after the fingers lifted"
    );
}
