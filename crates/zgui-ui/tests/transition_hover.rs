//! A change to `transition` alone draws what a full repaint draws.
//!
//! Such a change costs no layout and no repaint. Its one effect now is to stop a running transition
//! whose property leaves `transition-property`: the value jumps to its end. That jump is a change
//! of the property's own value, and it must reach the screen like any other.
//!
//! Each fixture reads the composed target with the real damage set and compares it with a full
//! repaint of the same moment.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::NodeRef;

use crate::painted::stage::Stage;

/// A swatch whose colour transition a class stops, and a row whose hover rule stops the transition
/// it leaves with.
const SHEET: &str = ":root { background-color: #ffffff }
     .stage { display: flex; flex-direction: row; gap: 10px; padding: 24px }
     .swatch, .row { width: 48px; height: 20px; background-color: #000000;
                     transition: background-color 400ms linear }
     .swatch.on { background-color: #ff0000 }
     .swatch.cut { transition-property: none }
     .row:hover { background-color: #00ff00; transition: none }";

/// How many output frames to run a transition for before it is stopped: about a third of it.
const PART: usize = 8;

/// An open stage, with its swatch, its row and the two switches on the swatch.
struct Opened {
    stage: Stage,
    swatch: NodeRef,
    row: NodeRef,
    on: RwSignal<bool>,
    cut: RwSignal<bool>,
}

fn open() -> Option<Opened> {
    crate::device::use_real_damage();
    let (swatch, row) = (NodeRef::new(), NodeRef::new());
    let (on, cut) = (RwSignal::new(false), RwSignal::new(false));
    let stage = Stage::open(SHEET, move || {
        view! {
            row(class = "stage") {
                box(
                    class = "swatch",
                    class:on = move || on.get(),
                    class:cut = move || cut.get(),
                    node_ref = swatch,
                )
                box(class = "row", node_ref = row)
            }
        }
    })?;
    Some(Opened {
        stage,
        swatch,
        row,
        on,
        cut,
    })
}

impl Opened {
    /// The border box of `node`, on the surface.
    fn rect(&self, node: NodeRef) -> Rect<DevicePx, Device> {
        self.stage
            .rect_of(node.get_untracked().expect("the box was built"))
    }

    /// The colours of the whole stage on the composed target.
    fn stage_colours(&self) -> Vec<(u8, u8, u8)> {
        self.stage.composed_colours_in(Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(200.0), DevicePx(80.0)),
        ))
    }

    /// The one colour `node` shows on the composed target.
    ///
    /// # Panics
    ///
    /// Panics when the box shows more than one colour.
    fn colour_of(&self, node: NodeRef) -> (u8, u8, u8) {
        let colours = self.stage.composed_colours_in(self.rect(node));
        let first = colours[0];
        assert!(
            colours.iter().all(|colour| *colour == first),
            "the box shows more than one colour"
        );
        first
    }

    /// Asserts that the composed target is what a full repaint draws.
    fn assert_as_repainted(&mut self, moment: &str) {
        let live = self.stage_colours();
        self.stage.repaint();
        let whole = self.stage_colours();
        let differ = live.iter().zip(&whole).filter(|(a, b)| a != b).count();
        assert_eq!(differ, 0, "{moment}: {differ} pixels differ from a repaint");
    }

    /// Runs `frames` output frames.
    fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.stage.tick();
        }
    }
}

/// Whether a colour is part of the way from black to `to` on one channel.
fn between(colour: (u8, u8, u8), channel: fn((u8, u8, u8)) -> u8) -> bool {
    let value = channel(colour);
    value > 0 && value < 255
}

#[test]
fn a_class_that_stops_a_transition_jumps_to_its_end() {
    let Some(mut opened) = open() else {
        return;
    };
    opened.on.set(true);
    opened.run(PART);
    let part = opened.colour_of(opened.swatch);
    assert!(
        between(part, |colour| colour.0),
        "the transition has to be running when it is stopped: {part:?}"
    );

    opened.cut.set(true);
    opened.run(2);
    assert_eq!(
        opened.colour_of(opened.swatch),
        (255, 0, 0),
        "the stopped transition jumps to its end"
    );
    opened.assert_as_repainted("after the stop");
    opened.run(30);
    assert_eq!(opened.colour_of(opened.swatch), (255, 0, 0));
    opened.assert_as_repainted("at rest");
}

#[test]
fn a_hover_that_stops_the_leaving_transition_jumps_to_its_colour() {
    let Some(mut opened) = open() else {
        return;
    };
    let row = opened.rect(opened.row);
    let on_row = Point::new(
        DevicePx(row.origin.x.0 + row.size.width.0 / 2.0),
        DevicePx(row.origin.y.0 + row.size.height.0 / 2.0),
    );
    opened.stage.move_to(on_row);
    opened.run(2);
    assert_eq!(
        opened.colour_of(opened.row),
        (0, 255, 0),
        "the hover sets its colour at once"
    );
    opened.assert_as_repainted("under the pointer");

    opened
        .stage
        .move_to(Point::new(DevicePx(400.0), DevicePx(400.0)));
    opened.run(PART);
    let part = opened.colour_of(opened.row);
    assert!(
        between(part, |colour| colour.1),
        "the leaving transition has to be running when the pointer comes back: {part:?}"
    );

    opened.stage.move_to(on_row);
    opened.run(2);
    assert_eq!(
        opened.colour_of(opened.row),
        (0, 255, 0),
        "the hover stops the leaving transition and its colour shows at once"
    );
    opened.assert_as_repainted("back under the pointer");
}
