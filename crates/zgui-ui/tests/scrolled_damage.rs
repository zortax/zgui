//! A scroll answered by a pixel move leaves no stale ink of what changed in the same frame.
//!
//! A scroll frame that moves a port's composed pixels draws only the band the move exposes and
//! whatever else the frame damaged. Everything else in the port is the previous frame's pixels,
//! moved. So a box that changed in that frame (a row that lost the hover, a sliding stripe, a
//! colour or an opacity step) must be damaged where the move put its old pixels, or they stay.
//!
//! Each fixture reads the composed target with the real damage set after every frame and checks
//! the boxes against what they can show at that moment.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{AnyView, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// A sidebar: an opaque column with a scroll area of ghost rows that light up under the pointer.
const SHEET: &str = ":root { background-color: #ffffff }
    .page { padding: 20px }
    .side { width: 260px; height: 400px; background-color: #1a1a1a; flex-direction: column }
    .scroll { flex: 1 1 0; min-height: 0 }
    .rows { padding: 0 8px 12px; gap: 2px; flex-direction: column }
    .row { flex: none; height: 24px; border-radius: 6px }
    .row:hover { background-color: #3060ff }
    .row.eased { transition: background-color 90ms linear }
    .bar { flex: none; position: relative; overflow: hidden; height: 16px; width: 100%;
           background-color: #304030 }
    .fill { height: 100%; width: 40%; background-color: #40ff40;
            transform: translateX(0); animation: slide 1200ms ease-in-out infinite }
    @keyframes slide { 0% { transform: translateX(-100%) } 100% { transform: translateX(250%) } }
    .tint { flex: none; height: 24px; background-color: #800000;
            transition: background-color 200ms steps(4, jump-end) }
    .tint.on { background-color: #ff8080 }
    .fade { flex: none; height: 24px; background-color: #e0e0e0;
            transition: opacity 200ms steps(4, jump-end) }
    .fade.on { opacity: 0.25 }
    .slot { flex: none; padding: 4px 0; pointer-events: none }
    .still { flex: none; height: 24px; border-radius: 6px; background-color: #2a2a2a }";

/// The sidebar's own colour, which a ghost row shows at rest.
const SIDE: (u8, u8, u8) = (0x1a, 0x1a, 0x1a);

/// A row under the pointer.
const HOVER: (u8, u8, u8) = (0x30, 0x60, 0xff);

/// The sliding stripe of a bar.
const FILL: (u8, u8, u8) = (0x40, 0xff, 0x40);

/// How many items the list holds.
const ITEMS: usize = 40;

/// Which item every fourth row is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    /// Every row is a ghost row.
    Rows,
    /// Every row is a ghost row whose hover colour eases in and out.
    EasedRows,
    /// An indeterminate bar.
    Bar,
    /// A box whose background colour moves.
    Tint,
    /// A box whose opacity moves.
    Fade,
    /// The library's indeterminate progress bar, under the library's theme.
    Progress,
    /// The library's text field, under the library's theme.
    Field,
}

/// An open sidebar and the nodes the checks read.
struct Opened {
    stage: Stage,
    side: NodeRef,
    rows: Vec<NodeRef>,
    specials: Vec<NodeRef>,
}

/// Opens the sidebar with `item` in every fourth row, switched by `on`.
fn open(item: Item, on: RwSignal<bool>) -> Option<Opened> {
    crate::device::use_real_damage();
    let side = NodeRef::new();
    let rows: Vec<NodeRef> = (0..ITEMS).map(|_| NodeRef::new()).collect();
    let refs = rows.clone();
    let special =
        move |index: usize| !matches!(item, Item::Rows | Item::EasedRows) && index % 4 == 1;
    // Rows that light up would make a repaint differ by the hover the next frame applies.
    let row = match item {
        Item::EasedRows => "row eased",
        Item::Progress | Item::Field => "still",
        _ => "row",
    };
    let stage = Stage::open(SHEET, move || {
        let children: Vec<AnyView> = refs
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let node = *node;
                if !special(index) {
                    return AnyView::new(view! { box(class = row, node_ref = node) });
                }
                match item {
                    Item::Bar => AnyView::new(view! {
                        box(class = "bar", node_ref = node) { box(class = "fill") }
                    }),
                    Item::Progress => AnyView::new(view! {
                        box(class = "slot", node_ref = node) { Progress() }
                    }),
                    Item::Field => AnyView::new(view! {
                        box(class = "slot") { Input(node_ref = node, placeholder = "Name") }
                    }),
                    Item::Tint => AnyView::new(view! {
                        box(class = "tint", class:on = move || on.get(), node_ref = node)
                    }),
                    _ => AnyView::new(view! {
                        box(class = "fade", class:on = move || on.get(), node_ref = node)
                    }),
                }
            })
            .collect();
        let page = AnyView::new(view! {
            column(class = "page") {
                column(class = "side", node_ref = side) {
                    ScrollArea(class = "scroll") {
                        column(class = "rows") { {children} }
                    }
                }
            }
        });
        if matches!(item, Item::Progress | Item::Field) {
            AnyView::new(view! { ThemeProvider { {page} } })
        } else {
            page
        }
    })?;
    let (specials, rows) = rows
        .into_iter()
        .enumerate()
        .partition::<Vec<_>, _>(|(index, _)| special(*index));
    Some(Opened {
        stage,
        side,
        rows: rows.into_iter().map(|(_, node)| node).collect(),
        specials: specials.into_iter().map(|(_, node)| node).collect(),
    })
}

/// One way of scrolling, one frame at a time.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// A wheel detent, carried over the frames that follow.
    Lines(f32),
    /// A touchpad movement of this many CSS pixels, applied at once.
    Pixels(f32),
    /// No input: a frame of whatever is already moving.
    Tick,
}

/// Wheel detents down and up with the glide frames between, then touchpad steps of whole and
/// fractional pixels.
fn steps() -> Vec<Step> {
    let mut steps = Vec::new();
    for lines in [1.0, 1.0, 2.0, -1.0, 1.0, -2.0] {
        steps.push(Step::Lines(lines));
        steps.extend(std::iter::repeat_n(Step::Tick, 9));
    }
    for pixels in [7.0, 7.0, 13.0, -6.0, 2.5, 2.5, 3.0, -9.5, 24.0, -24.0] {
        steps.push(Step::Pixels(pixels));
        steps.push(Step::Tick);
    }
    steps
}

impl Opened {
    /// Where `node` is on the surface now.
    fn rect(&self, node: NodeRef) -> Rect<DevicePx, Device> {
        self.stage
            .rect_of(node.get_untracked().expect("the box was built"))
    }

    /// The part of `rect` inside the sidebar, shrunk by `inset` on every side.
    fn inside(&self, rect: Rect<DevicePx, Device>, inset: f32) -> Option<Rect<DevicePx, Device>> {
        let side = self.rect(self.side);
        let cut = rect.intersection(side)?;
        let width = cut.size.width.0 - 2.0 * inset;
        let height = cut.size.height.0 - 2.0 * inset;
        (width >= 1.0 && height >= 1.0).then(|| {
            Rect::new(
                Point::new(
                    DevicePx(cut.origin.x.0 + inset),
                    DevicePx(cut.origin.y.0 + inset),
                ),
                Size::new(DevicePx(width), DevicePx(height)),
            )
        })
    }

    /// Runs one step, and says whether its frame moved composed pixels.
    fn step(&mut self, step: Step) -> bool {
        let before = crate::device::shifts();
        match step {
            Step::Lines(lines) => self.stage.wheel_step(lines),
            Step::Pixels(pixels) => self.stage.touchpad_step(pixels),
            Step::Tick => self.stage.tick(),
        }
        crate::device::shifts() > before
    }

    /// Puts the pointer over the list.
    fn aim(&mut self) {
        let side = self.rect(self.side);
        self.stage.move_to(Point::new(
            DevicePx(side.origin.x.0 + 100.0),
            DevicePx(side.origin.y.0 + 150.0),
        ));
    }
}

/// How many pixels inside `rect` differ from its commonest colour, and that colour.
fn spread(colours: &[(u8, u8, u8)]) -> (usize, (u8, u8, u8)) {
    let mut counts: Vec<((u8, u8, u8), usize)> = Vec::new();
    for colour in colours {
        match counts.iter_mut().find(|(seen, _)| seen == colour) {
            Some((_, count)) => *count += 1,
            None => counts.push((*colour, 1)),
        }
    }
    let (common, most) = counts
        .iter()
        .copied()
        .max_by_key(|(_, count)| *count)
        .unwrap_or((SIDE, 0));
    (colours.len() - most, common)
}

/// Runs every step of [`steps`], toggling `on` every seventh frame, and gathers what `check`
/// finds wrong after each frame.
///
/// # Panics
///
/// Panics when fewer than ten frames moved composed pixels, because the fixture would then test the
/// full redraw of the port and nothing else.
fn run(
    opened: &mut Opened,
    on: RwSignal<bool>,
    mut check: impl FnMut(&mut Opened) -> Vec<String>,
) -> Vec<String> {
    opened.aim();
    let mut faults = Vec::new();
    let mut shifts = 0;
    for (frame, step) in steps().into_iter().enumerate() {
        if frame % 7 == 0 {
            on.set(!on.get_untracked());
        }
        let shifted = opened.step(step);
        shifts += usize::from(shifted);
        let moved = if shifted { ", shifted" } else { "" };
        faults.extend(
            check(opened)
                .into_iter()
                .map(|fault| format!("frame {frame} ({step:?}{moved}): {fault}")),
        );
    }
    assert!(shifts >= 10, "only {shifts} frames moved composed pixels");
    faults
}

/// Fails when any frame shows a ghost row in two colours, or more than one row lit.
#[test]
fn rows_that_scroll_from_under_the_pointer_lose_their_hover() {
    hover_follows_the_pointer(Item::Rows);
}

/// The same, for rows whose hover colour eases in and out.
#[test]
fn eased_rows_that_scroll_from_under_the_pointer_lose_their_hover() {
    hover_follows_the_pointer(Item::EasedRows);
}

/// Fails when any frame shows a row of `item` in two colours, or more than one row lit.
fn hover_follows_the_pointer(item: Item) {
    let on = RwSignal::new(false);
    let Some(mut opened) = open(item, on) else {
        return;
    };
    let faults = run(&mut opened, on, |opened| {
        let mut faults = Vec::new();
        let mut lit = 0;
        for (index, row) in opened.rows.iter().enumerate() {
            let Some(inner) = opened.inside(opened.rect(*row), 3.0) else {
                continue;
            };
            let colours = opened.stage.composed_colours_in(inner);
            let (off, common) = spread(&colours);
            if common == HOVER {
                lit += 1;
            }
            if off > 0 || (item == Item::Rows && common != SIDE && common != HOVER) {
                faults.push(format!(
                    "row {index} shows {off} of {} pixels off {common:?}",
                    colours.len()
                ));
            }
        }
        if lit > 1 {
            faults.push(format!("{lit} rows lit"));
        }
        faults
    });
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// Fails when any frame shows a bar's stripe outside the bar, or a bar whose lines disagree about
/// where the stripe is.
#[test]
fn a_sliding_bar_in_a_scrolled_list_leaves_no_stripe_behind() {
    let on = RwSignal::new(false);
    let Some(mut opened) = open(Item::Bar, on) else {
        return;
    };
    let faults = run(&mut opened, on, |opened| {
        let mut faults = Vec::new();
        let side = opened
            .inside(opened.rect(opened.side), 0.0)
            .expect("the side shows");
        let bars: Vec<_> = opened
            .specials
            .iter()
            .map(|bar| opened.rect(*bar))
            .collect();
        let colours = opened.stage.composed_colours_in(side);
        let top = side.origin.y.0;
        let width = side.size.width.0 as usize;
        let stray = colours
            .chunks(width)
            .enumerate()
            .filter(|(line, _)| {
                let y = top + *line as f32;
                !bars.iter().any(|bar| {
                    y >= bar.origin.y.0 - 1.0 && y < bar.origin.y.0 + bar.size.height.0 + 1.0
                })
            })
            .map(|(_, pixels)| pixels.iter().filter(|colour| **colour == FILL).count())
            .sum::<usize>();
        if stray > 0 {
            faults.push(format!("{stray} stripe pixels outside every bar"));
        }
        for (index, bar) in bars.iter().enumerate() {
            let Some(inner) = opened.inside(*bar, 0.0) else {
                continue;
            };
            let inner = Rect::new(
                Point::new(inner.origin.x, DevicePx(inner.origin.y.0 + 1.0)),
                Size::new(inner.size.width, DevicePx(inner.size.height.0 - 2.0)),
            );
            if inner.size.height.0 < 1.0 {
                continue;
            }
            let colours = opened.stage.composed_colours_in(inner);
            let mut lines = colours.chunks(inner.size.width.0 as usize);
            let first = lines.next().unwrap_or_default();
            let differing = lines.filter(|line| *line != first).count();
            if differing > 0 {
                faults.push(format!(
                    "bar {index}: {differing} lines differ from its first"
                ));
            }
        }
        faults
    });
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

/// Fails when a box of `item` shows two levels at once in any frame.
fn one_level_per_frame(item: Item) {
    let on = RwSignal::new(false);
    let Some(mut opened) = open(item, on) else {
        return;
    };
    let faults = run(&mut opened, on, |opened| {
        let mut faults = Vec::new();
        for (index, special) in opened.specials.iter().enumerate() {
            let Some(inner) = opened.inside(opened.rect(*special), 2.0) else {
                continue;
            };
            let colours = opened.stage.composed_colours_in(inner);
            let (off, common) = spread(&colours);
            if off > 0 {
                faults.push(format!(
                    "box {index} shows {off} of {} pixels off {common:?}",
                    colours.len()
                ));
            }
        }
        faults
    });
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn a_colour_step_in_a_scrolled_list_shows_one_colour() {
    one_level_per_frame(Item::Tint);
}

#[test]
fn an_opacity_step_in_a_scrolled_list_shows_one_level() {
    one_level_per_frame(Item::Fade);
}

/// Fails when any frame of the library's indeterminate bar in a scrolled list differs from a full
/// repaint of the same moment.
#[test]
fn the_library_bar_in_a_scrolled_list_matches_a_repaint() {
    matches_a_repaint(Item::Progress);
}

/// Fails when any frame of a blinking caret in a scrolled list differs from a full repaint of the
/// same moment.
#[test]
fn a_blinking_caret_in_a_scrolled_list_matches_a_repaint() {
    matches_a_repaint(Item::Field);
}

/// Fails when any frame of a list of `item` differs from a full repaint of the same moment.
///
/// Its rows do not light up and its items take no pointer: a hover the next frame applies would
/// make the repaint differ.
fn matches_a_repaint(item: Item) {
    let on = RwSignal::new(false);
    let Some(mut opened) = open(item, on) else {
        return;
    };
    if item == Item::Field {
        // A field in the middle of the list, so its caret scrolls through the port.
        opened.specials[2].focus();
        opened.stage.wait(std::time::Duration::from_millis(400));
    }
    let faults = run(&mut opened, on, |opened| {
        let side = opened
            .inside(opened.rect(opened.side), 0.0)
            .expect("the side shows");
        let live = opened.stage.composed_colours_in(side);
        opened.stage.repaint();
        let whole = opened.stage.composed_colours_in(side);
        let width = side.size.width.0 as usize;
        let mut differing = 0;
        let mut bounds = (usize::MAX, usize::MAX, 0, 0);
        for (index, _) in live
            .iter()
            .zip(&whole)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
        {
            differing += 1;
            let (x, y) = (index % width, index / width);
            bounds = (
                bounds.0.min(x),
                bounds.1.min(y),
                bounds.2.max(x),
                bounds.3.max(y),
            );
        }
        if differing > 0 {
            vec![format!(
                "{differing} pixels differ in {bounds:?} of the side"
            )]
        } else {
            Vec::new()
        }
    });
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
