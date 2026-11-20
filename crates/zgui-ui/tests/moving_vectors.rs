//! Vector drawings under a moving transform repaint to the same picture as a full redraw.
//!
//! A node graph pans, zooms and drags boxes that stroked paths connect, and labels sit on top of
//! the paths. Each partial repaint is read off the composed target and compared with a complete
//! redraw of the same document.

mod desktop;
mod device;
mod painted;

use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;
use zgui::vocab::{PropValue, SharedString};

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
    .pane { position: relative; width: 800px; height: 600px; overflow: hidden }
    .layer { position: absolute; left: 0; top: 0; width: 0; height: 0; transform-origin: 0 0 }
    .edge { position: absolute; left: 0; top: 0; color: transparent;
            --zgui-stroke: #6b7280; --zgui-stroke-width: 2px }
    .node { position: absolute; left: 0; top: 0; width: 120px; height: 44px;
            background-color: #f4f4f5; border: 1px solid #d4d4d8; border-radius: 8px }
    .label { position: absolute; left: 0; top: 0; padding: 2px 6px; background-color: #ffffff;
             border-radius: 4px; font-size: 12px }";

/// The positions the scene draws from.
#[derive(Clone, Copy)]
struct Scene {
    pan: RwSignal<(f32, f32)>,
    zoom: RwSignal<f32>,
    dragged: RwSignal<(f32, f32)>,
}

const FIXED: [(f32, f32); 3] = [(60.0, 60.0), (420.0, 80.0), (380.0, 360.0)];

/// A cubic path from the right side of `a` to the left side of `b`, in the layer's space, with its
/// box origin.
fn edge(a: (f32, f32), b: (f32, f32)) -> ((f32, f32), (f32, f32), String) {
    let (sx, sy) = (a.0 + 120.0, a.1 + 22.0);
    let (tx, ty) = (b.0, b.1 + 22.0);
    let pad = 8.0;
    let (left, top) = (sx.min(tx) - pad, sy.min(ty) - pad);
    let (right, bottom) = (sx.max(tx) + pad, sy.max(ty) + pad);
    let mid = (sx + tx) / 2.0;
    let d = format!(
        "M{} {} C{} {} {} {} {} {}",
        sx - left,
        sy - top,
        mid - left,
        sy - top,
        mid - left,
        ty - top,
        tx - left,
        ty - top
    );
    ((left, top), (right - left, bottom - top), d)
}

fn graph(scene: Scene) -> impl Fn() -> AnyView {
    move || {
        let node_at = move |index: usize| {
            move || {
                let at = if index == 0 {
                    scene.dragged.get()
                } else {
                    FIXED[index - 1]
                };
                Some(format!("translate({}px, {}px)", at.0, at.1))
            }
        };
        let path = move |from: usize, to: usize| {
            let at = move |index: usize| {
                if index == 0 {
                    scene.dragged.get()
                } else {
                    FIXED[index - 1]
                }
            };
            let geometry = move || edge(at(from), at(to));
            view! {
                vector(
                    class = "edge",
                    style:transform = move || { let ((x, y), _, _) = geometry(); Some(format!("translate({x}px, {y}px)")) },
                    style:width = move || { let (_, (w, _), _) = geometry(); Some(format!("{w}px")) },
                    style:height = move || { let (_, (_, h), _) = geometry(); Some(format!("{h}px")) },
                    prop:d = move || { let (_, _, d) = geometry(); PropValue::from(SharedString::from(d)) }
                ) {}
            }
        };
        let label = move || {
            let a = scene.dragged.get();
            let b = FIXED[0];
            let x = (a.0 + 120.0 + b.0) / 2.0 - 16.0;
            let y = (a.1 + b.1) / 2.0 + 14.0;
            Some(format!("translate({x}px, {y}px)"))
        };
        AnyView::new(view! {
            box(class = "pane") {
                box(
                    class = "layer",
                    style:transform = move || {
                        let (x, y) = scene.pan.get();
                        Some(format!("translate({x}px, {y}px) scale({})", scene.zoom.get()))
                    }
                ) {
                    {path(0, 1)}
                    {path(1, 2)}
                    {path(0, 3)}
                    {path(3, 2)}
                    box(class = "node", style:transform = node_at(0)) {}
                    box(class = "node", style:transform = node_at(1)) {}
                    box(class = "node", style:transform = node_at(2)) {}
                    box(class = "node", style:transform = node_at(3)) {}
                    box(class = "label", style:transform = label) {"rows"}
                }
            }
        })
    }
}

/// Every pixel of the composed target inside the pane.
fn picture(stage: &Stage) -> Vec<(u8, u8, u8)> {
    use zgui::geom::{Device, DevicePx, Point, Rect, Size};
    stage.composed_colours_in(Rect::<DevicePx, Device>::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(800.0), DevicePx(600.0)),
    ))
}

/// How many pixels differ by more than a small tolerance.
fn differing(a: &[(u8, u8, u8)], b: &[(u8, u8, u8)]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(x, y)| {
            let d = |p: u8, q: u8| (i32::from(p) - i32::from(q)).abs();
            d(x.0, y.0) + d(x.1, y.1) + d(x.2, y.2) > 24
        })
        .count()
}

#[test]
fn partial_repaints_of_a_moving_graph_match_a_full_redraw() {
    crate::device::use_real_damage();
    let scene = Scene {
        pan: RwSignal::new((20.0, 10.0)),
        zoom: RwSignal::new(1.0),
        dragged: RwSignal::new((100.0, 240.0)),
    };
    let Some(mut stage) = Stage::open(SHEET, graph(scene)) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.wait_quietly(std::time::Duration::from_millis(34));
    let mut failures = Vec::new();
    let mut check = |stage: &mut Stage, step: &str| {
        let partial = picture(stage);
        stage.repaint();
        let full = picture(stage);
        let wrong = differing(&partial, &full);
        if wrong > 0 {
            stage.capture_composed(&format!("moving-{step}"));
            failures.push(format!("{step}: {wrong} pixels"));
        }
    };
    for step in 0..24 {
        let t = step as f32;
        scene.dragged.set((100.0 + t * 9.0, 240.0 - t * 6.5));
        stage.wait_quietly(std::time::Duration::from_millis(34));
        check(&mut stage, &format!("drag-{step}"));
    }
    for step in 0..16 {
        let t = step as f32;
        scene.pan.set((20.0 - t * 7.0, 10.0 + t * 5.0));
        stage.wait_quietly(std::time::Duration::from_millis(34));
        check(&mut stage, &format!("pan-{step}"));
    }
    for step in 0..16 {
        let t = step as f32;
        scene.zoom.set(1.0 + t * 0.043);
        stage.wait_quietly(std::time::Duration::from_millis(34));
        check(&mut stage, &format!("zoom-{step}"));
    }
    assert!(
        failures.is_empty(),
        "partial repaints differ from a full redraw: {failures:#?}"
    );
}
