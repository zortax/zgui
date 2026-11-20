//! What is kept of each frame the application draws.

use std::sync::Arc;

use zgui::geom::{Device, Point, Rect, Size};
use zgui_testkit_compositor::{Drawing, Instance, Pixels};

use crate::raster::page::INK;

/// One frame of the application.
pub struct Recorded {
    /// The target as the frame left it.
    pub full: Arc<Pixels>,
    /// Where the drawing's items claimed their ink would land, in whole device pixels.
    pub declared: Option<Rect<i32, Device>>,
    /// How many frame paths the frame drew, each of which the path rasteriser fills.
    pub paths: usize,
    /// How many items in the drawing's colour the frame drew.
    pub items: usize,
}

/// Keeps what `drawing` drew and the target `full` it left.
pub fn record(drawing: &Drawing, full: Arc<Pixels>) -> Recorded {
    let shapes: Vec<[f32; 4]> = drawing
        .instances()
        .iter()
        .filter(|instance| inked(instance))
        .map(|instance| instance.rect("ink").unwrap_or_else(|| instance.bounds()))
        .collect();
    Recorded {
        full,
        declared: ink(&shapes),
        paths: drawing.of_kind("path").count(),
        items: shapes.len(),
    }
}

/// Reports whether `instance` paints in the drawing's colour.
fn inked(instance: &Instance) -> bool {
    ["fill", "paint", "stroke"].iter().any(|field| {
        instance.color(field).is_some_and(|color| {
            color
                .iter()
                .zip(INK)
                .all(|(channel, ink)| (channel * 255.0 - f32::from(ink)).abs() <= 1.0)
        })
    })
}

/// The whole-pixel rectangle around `shapes`, or `None` when there are none.
fn ink(shapes: &[[f32; 4]]) -> Option<Rect<i32, Device>> {
    let left = shapes.iter().map(|shape| shape[0]).reduce(f32::min)?;
    let top = shapes.iter().map(|shape| shape[1]).reduce(f32::min)?;
    let right = shapes
        .iter()
        .map(|shape| shape[0] + shape[2])
        .reduce(f32::max)?;
    let bottom = shapes
        .iter()
        .map(|shape| shape[1] + shape[3])
        .reduce(f32::max)?;
    let (left, top) = (left.floor() as i32, top.floor() as i32);
    let (right, bottom) = (right.ceil() as i32, bottom.ceil() as i32);
    Some(Rect::new(
        Point::new(left, top),
        Size::new(right - left, bottom - top),
    ))
}
