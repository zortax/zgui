//! A scrolled sidebar whose rows are made again on hover leaves no stale tone.
//!
//! The rows change tone and `transition` under the pointer. In the variants they also gain
//! generated content, and the cascade makes a row's boxes again for that. So a row that scrolls out
//! from under the pointer loses its pieces in a frame that moves the port's pixels. The old pieces
//! name their pixels where the drawn frame put them, and the move puts those pixels somewhere else.
//! Damage left at the old place leaves a band of the old tone as tall as the move.
//!
//! Each fixture scrolls the way a wheel and a touchpad do, at the scales a desktop uses, with the
//! real damage set. It lets every gesture come to rest without a repaint, and then compares the
//! composed target with a full repaint of the same moment.

mod desktop;
mod device;
mod painted;

use std::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{AnyView, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// A dark sidebar: a band at the top, groups of ghost rows in a scroll area, and rows at the foot.
const SHEET: &str = "
    :root { background-color: #161617; color: #c9c7cd; font-size: 13px; line-height: 20px }
    .frame { width: 100%; height: 100% }
    .body { flex-direction: row; width: 100%; height: 100% }
    .page { flex: 1 1 0; background-color: #161617 }
    .side { flex: 0 0 auto; width: 248px; height: 100%; min-height: 0;
            background-color: #121213; color: #b4b1ba }
    .side__top { flex: 0 0 auto; height: 40px; padding: 0 6px 0 16px; align-items: center }
    .side__scroll { flex: 1 1 0; min-height: 0 }
    .side__groups { padding: 0 8px 12px }
    .side__foot { flex: 0 0 auto; gap: 1px; padding: 8px }
    .group { margin-top: 4px }
    .group + .group { margin-top: 14px }
    .group__title { align-items: center; height: 26px; padding: 0 2px 0 8px }
    .group__fold { display: flex; align-items: center; gap: 4px; height: 22px; font-size: 11px }
    .group__chevron { opacity: 0; transition: opacity 60ms }
    .group__title:hover .group__chevron { opacity: 1 }
    .group__day { height: 24px; padding: 6px 8px 0; font-size: 11px }
    .grow { flex: 1 1 auto; min-width: 0 }
    .icon { display: inline-block; flex: none; pointer-events: none; width: 12px; height: 12px }
    .slot { position: relative; display: flex; align-items: center }
    .row { display: flex; flex: 1 1 auto; align-items: center; gap: 10px; min-width: 0;
           height: 28px; padding: 0 8px; border-radius: 8px; cursor: pointer }
    .row:hover { background-color: color-mix(in oklab, #1f1f21 80%, transparent);
                 transition: none }
    .row__label { flex: 1 1 auto; min-width: 0; overflow: hidden; white-space: nowrap;
                  text-overflow: ellipsis }
    .row__meta { flex: 0 0 auto; font-size: 11px }
    .hue { flex: 0 0 auto; width: 8px; height: 8px; border-radius: 50%;
           background-color: #92a2d5 }
    .key { position: absolute; right: 3px; display: flex; align-items: center;
           justify-content: center; width: 22px; height: 22px; border-radius: 8px;
           opacity: 0; transition: opacity 60ms }
    .slot:hover .key { opacity: 1 }
    .key:hover { background-color: #232326 }";

/// A stroked icon.
const LINES: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M2 10v3"/><path d="M6 6v11"/><path d="M10 3v18"/><path d="M14 8v7"/><path d="M18 5v13"/><path d="M22 10v3"/></svg>"#;

/// Another, for the group titles.
const CHEVRON: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="m6 9 6 6 6-6"/></svg>"#;

/// How many rows have a mark and a key.
const MARKED: usize = 5;

/// How many rows open a menu.
const MENUS: usize = 40;

/// What the rows hold.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rows {
    /// Words, a time, marks and keys, as a sidebar shows them.
    Labelled,
    /// Nothing, so a row shows one tone when nothing is stale.
    Bare,
}

fn icon(svg: &'static str, class: &'static str) -> AnyView {
    AnyView::new(view! {
        vector(class = "icon", class = class, prop:svg = PropValue::from(svg), a11y:hidden = true)
    })
}

/// A key at the trailing end of a row, shown under the pointer, with a tooltip.
fn key() -> AnyView {
    AnyView::new(view! {
        Tooltip {
            TooltipTrigger {
                control(class = "key", tabindex = Focus::Sequential, a11y:role = Role::Button) {
                    {icon(LINES, "")}
                }
            }
            TooltipContent {"Start"}
        }
    })
}

fn title(label: &'static str) -> AnyView {
    AnyView::new(view! {
        row(class = "group__title") {
            control(class = "group__fold", tabindex = Focus::Sequential, a11y:role = Role::Button) {
                text {{label}}
                {icon(CHEVRON, "group__chevron")}
            }
        }
    })
}

/// A row with a mark, a name and a key.
fn marked_row(index: usize, rows: Rows, node: NodeRef) -> AnyView {
    if rows == Rows::Bare {
        return AnyView::new(view! {
            row(class = "slot") {
                control(class = "row", node_ref = node, tabindex = Focus::Sequential)
            }
        });
    }
    let name = format!("Item number {index}");
    AnyView::new(view! {
        row(class = "slot") {
            control(class = "row", node_ref = node, tabindex = Focus::Sequential, a11y:role = Role::Link) {
                box(class = "hue")
                text(class = "row__label") {{name}}
            }
            {key()}
        }
    })
}

/// A row with a name and a time, which opens a menu.
fn menu_row(index: usize, rows: Rows, node: NodeRef) -> AnyView {
    let inner = if rows == Rows::Bare {
        AnyView::new(
            view! { control(class = "row", node_ref = node, tabindex = Focus::Sequential) },
        )
    } else {
        let name = format!("A conversation about topic {index} that runs long");
        let when = format!("{:02}:{:02}", index % 24, (index * 7) % 60);
        AnyView::new(view! {
            control(class = "row", node_ref = node, tabindex = Focus::Sequential, a11y:role = Role::Link) {
                text(class = "row__label") {{name}}
                text(class = "row__meta") {{when}}
            }
        })
    };
    AnyView::new(view! {
        ContextMenu {
            ContextMenuTrigger {
                row(class = "slot") { {inner} }
            }
            ContextMenuContent {
                MenuItem { text {"Rename"} }
            }
        }
    })
}

/// The sidebar, with the scroll area recorded in `scroll` and the rows in `nodes`.
fn sidebar(scroll: NodeRef, rows: Rows, nodes: &[NodeRef]) -> AnyView {
    let marked: Vec<AnyView> = nodes[..MARKED]
        .iter()
        .enumerate()
        .map(|(index, node)| marked_row(index, rows, *node))
        .collect();
    let mut menus: Vec<AnyView> = Vec::new();
    for (index, node) in nodes[MARKED..].iter().enumerate() {
        if index % 9 == 0 {
            menus.push(AnyView::new(
                view! { text(class = "group__day") {"Yesterday"} },
            ));
        }
        menus.push(menu_row(index, rows, *node));
    }
    let feet: Vec<AnyView> = ["Tools", "Servers", "Settings"]
        .into_iter()
        .map(|label| {
            AnyView::new(view! {
                control(class = "row", tabindex = Focus::Sequential) {
                    {icon(LINES, "")}
                    text(class = "row__label") {{label}}
                }
            })
        })
        .collect();
    let first = title("Marked");
    let second = title("Menus");
    AnyView::new(view! {
        ThemeProvider(scheme = ColorScheme::Dark) {
            TooltipProvider(delay = Duration::from_millis(350), close_delay = Duration::ZERO) {
                stack(class = "frame") {
                    row(class = "body") {
                        column(class = "side") {
                            row(class = "side__top") { box(class = "grow") }
                            ScrollArea(class = "side__scroll", node_ref = scroll) {
                                column(class = "side__groups") {
                                    column(class = "group") { {first} column { {marked} } }
                                    column(class = "group") { {second} column { {menus} } }
                                }
                            }
                            column(class = "side__foot") { {feet} }
                        }
                        column(class = "page")
                    }
                }
            }
        }
    })
}

/// One way of scrolling.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// A wheel detent, carried over the frames that follow.
    Lines(f32),
    /// A touchpad movement of this many CSS pixels, applied at once.
    Pixels(f32),
}

/// Wheel detents down and up, then touchpad movements of whole and fractional pixels.
const GESTURES: [Step; 14] = [
    Step::Lines(1.0),
    Step::Lines(1.0),
    Step::Lines(2.0),
    Step::Lines(-1.0),
    Step::Lines(1.0),
    Step::Lines(-2.0),
    Step::Lines(3.0),
    Step::Pixels(7.0),
    Step::Pixels(13.0),
    Step::Pixels(-6.0),
    Step::Pixels(2.5),
    Step::Pixels(-9.5),
    Step::Pixels(24.0),
    Step::Pixels(-24.0),
];

/// How many frames a gesture runs before it is checked: a detent's glide is over by then.
const FRAMES: usize = 40;

/// An open sidebar and the region the checks read.
struct Opened {
    stage: Stage,
    /// The sidebar's part of the scroll area on the surface.
    area: Rect<DevicePx, Device>,
    /// Every row of the scroll area.
    rows: Vec<NodeRef>,
}

/// A hover rule that gives the row generated content, so a row's boxes are made again under the
/// pointer. The content is empty and out of flow, so it draws nothing.
const MADE_AGAIN: &str = ".row:hover::after { content: \"\"; position: absolute }";

/// Opens the sidebar at `scale` with the pointer resting over the rows, with `extra` added to the
/// sheet.
fn open(scale: f64, rows: Rows, extra: &str) -> Option<Opened> {
    crate::device::use_real_damage();
    let scroll = NodeRef::new();
    let nodes: Vec<NodeRef> = (0..MARKED + MENUS).map(|_| NodeRef::new()).collect();
    let held = nodes.clone();
    let mut stage = Stage::open(&format!("{SHEET}\n{extra}"), move || {
        sidebar(scroll, rows, &held)
    })?;
    if (scale - 1.0).abs() > f64::EPSILON {
        stage.rescale(scale);
    }
    stage.wait(Duration::from_millis(400));
    let port = stage.rect_of(scroll.get_untracked().expect("the scroll area was built"));
    let scale = scale as f32;
    let area = Rect::new(
        Point::new(DevicePx(0.0), port.origin.y),
        Size::new(DevicePx((248.0 * scale).floor()), port.size.height),
    );
    stage.move_to(Point::new(
        DevicePx(port.origin.x.0 + 100.0 * scale),
        DevicePx(port.origin.y.0 + 150.0 * scale),
    ));
    stage.wait(Duration::from_millis(400));
    Some(Opened {
        stage,
        area,
        rows: nodes,
    })
}

impl Opened {
    /// Runs one frame of `step`, or of whatever is already moving, and says whether it moved
    /// composed pixels.
    fn frame(&mut self, step: Option<Step>) -> bool {
        let before = crate::device::shifts();
        match step {
            Some(Step::Lines(lines)) => self.stage.wheel_step(lines),
            Some(Step::Pixels(pixels)) => self.stage.touchpad_step(pixels),
            None => self.stage.tick(),
        }
        crate::device::shifts() > before
    }

    /// How many pixels of the area differ from a full repaint, and the lines they are on.
    fn stale(&mut self) -> (usize, Option<(usize, usize)>) {
        let live = self.stage.composed_colours_in(self.area);
        self.stage.repaint();
        let whole = self.stage.composed_colours_in(self.area);
        let width = self.area.size.width.0 as usize;
        let mut count = 0;
        let mut lines: Option<(usize, usize)> = None;
        for (index, _) in live
            .iter()
            .zip(&whole)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
        {
            count += 1;
            let line = index / width;
            lines = Some(lines.map_or((line, line), |(top, bottom)| {
                (top.min(line), bottom.max(line))
            }));
        }
        (count, lines)
    }

    /// The rows in the area that show more than one tone, three pixels in from every side to stay
    /// clear of the rounded corners.
    fn rows_in_two_tones(&self) -> Vec<String> {
        let mut faults = Vec::new();
        for row in &self.rows {
            let Some(id) = row.get_untracked() else {
                continue;
            };
            let Some(cut) = self.stage.rect_of(id).intersection(self.area) else {
                continue;
            };
            let (width, height) = (cut.size.width.0 - 6.0, cut.size.height.0 - 6.0);
            if width < 1.0 || height < 1.0 {
                continue;
            }
            let inner = Rect::new(
                Point::new(
                    DevicePx(cut.origin.x.0 + 3.0),
                    DevicePx(cut.origin.y.0 + 3.0),
                ),
                Size::new(DevicePx(width), DevicePx(height)),
            );
            let colours = self.stage.composed_colours_in(inner);
            let first = colours.first().copied();
            let off = colours.iter().filter(|it| Some(**it) != first).count();
            if off > 0 {
                faults.push(format!(
                    "the row at {:.0} shows {off} of {} pixels in another tone",
                    inner.origin.y.0,
                    colours.len()
                ));
            }
        }
        faults
    }
}

/// Runs every gesture and lets it come to rest without a repaint, then compares the area with a
/// full repaint. With `each_frame`, also checks that every row shows one tone in every frame.
///
/// # Panics
///
/// Panics when a gesture leaves pixels that a repaint draws differently, when a row shows two
/// tones, and when too few frames moved composed pixels to test the move at all.
fn no_stale_tone(scale: f64, rows: Rows, extra: &str, each_frame: bool) {
    let Some(mut opened) = open(scale, rows, extra) else {
        return;
    };
    let mut faults = Vec::new();
    let mut shifts = 0;
    for (gesture, step) in GESTURES.into_iter().enumerate() {
        for frame in 0..FRAMES {
            shifts += usize::from(opened.frame((frame == 0).then_some(step)));
            if each_frame {
                faults.extend(
                    opened
                        .rows_in_two_tones()
                        .into_iter()
                        .map(|fault| format!("{step:?}, frame {frame}: {fault}")),
                );
            }
        }
        let (count, lines) = opened.stale();
        if count > 0 {
            faults.push(format!(
                "after gesture {gesture} ({step:?}), {count} pixels on lines {lines:?} differ \
                 from a repaint"
            ));
        }
    }
    assert!(shifts >= 20, "only {shifts} frames moved composed pixels");
    assert!(
        faults.is_empty(),
        "at scale {scale}:\n{}",
        faults.join("\n")
    );
}

#[test]
fn a_scrolled_sidebar_leaves_no_stale_tone_at_scale_1() {
    no_stale_tone(1.0, Rows::Labelled, "", false);
}

#[test]
fn a_scrolled_sidebar_leaves_no_stale_tone_at_scale_1_2() {
    no_stale_tone(1.2, Rows::Labelled, "", false);
}

#[test]
fn a_scrolled_sidebar_leaves_no_stale_tone_at_scale_1_25() {
    no_stale_tone(1.25, Rows::Labelled, "", false);
}

#[test]
fn a_scrolled_sidebar_leaves_no_stale_tone_at_scale_1_5() {
    no_stale_tone(1.5, Rows::Labelled, "", false);
}

#[test]
fn a_scrolled_sidebar_leaves_no_stale_tone_at_scale_2() {
    no_stale_tone(2.0, Rows::Labelled, "", false);
}

/// Bare rows, so every frame can be checked: a row shows its own tone or the sidebar's, never a
/// band of each.
#[test]
fn bare_rows_show_one_tone_in_every_frame_at_scale_1() {
    no_stale_tone(1.0, Rows::Bare, "", true);
}

#[test]
fn bare_rows_show_one_tone_in_every_frame_at_scale_1_2() {
    no_stale_tone(1.2, Rows::Bare, "", true);
}

/// Rows whose hover rule gives them generated content, so the rows under the pointer are made
/// again in frames that move the port's pixels.
#[test]
fn rows_made_again_leave_no_stale_tone_at_scale_1() {
    no_stale_tone(1.0, Rows::Labelled, MADE_AGAIN, false);
}

#[test]
fn rows_made_again_leave_no_stale_tone_at_scale_1_25() {
    no_stale_tone(1.25, Rows::Labelled, MADE_AGAIN, false);
}

#[test]
fn bare_rows_made_again_show_one_tone_in_every_frame_at_scale_1_2() {
    no_stale_tone(1.2, Rows::Bare, MADE_AGAIN, true);
}
