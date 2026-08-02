//! Group markers and backdrop filters: the two primitives that composite rather than paint.

use zgui_scene::peniko::BlendMode;
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Scene};

use crate::text::number::{float, rect};
use crate::transcript::paint;
use crate::transcript::primitive::suffix;

/// A group's opening or closing marker.
///
/// The read extent is printed only when it differs from what the group writes, which is the case
/// that costs damage expansion — every per-pixel filter, every blend mode and plain opacity read
/// exactly what they write, and printing `source=` on all of them would bury the one that matters.
pub fn group(scene: &Scene, boundary: &GroupBoundary) -> String {
    let mut line = format!(
        "{} order={} bounds={}",
        if boundary.is_start {
            "group_start"
        } else {
            "group_end"
        },
        boundary.order,
        rect([
            boundary.bounds.origin.x.0,
            boundary.bounds.origin.y.0,
            boundary.bounds.size.width.0,
            boundary.bounds.size.height.0,
        ])
    );
    if boundary.opacity != 1.0 {
        line.push_str(&format!(" opacity={}", float(boundary.opacity)));
    }
    if boundary.blend != BlendMode::default() {
        line.push_str(&format!(
            " blend={:?}/{:?}",
            boundary.blend.mix, boundary.blend.compose
        ));
    }
    if !boundary.filters.is_empty() {
        line.push_str(&format!(" filters={}", filters(&boundary.filters)));
    }
    if !boundary.reads_only_what_it_writes() {
        line.push_str(&format!(
            " source={}",
            rect([
                boundary.source.origin.x.0,
                boundary.source.origin.y.0,
                boundary.source.size.width.0,
                boundary.source.size.height.0,
            ])
        ));
    }
    line.push_str(&suffix(scene, boundary.clip, boundary.transform));
    line
}

/// A filter over the composite beneath it.
pub fn backdrop(scene: &Scene, backdrop: &BackdropFilter) -> String {
    let mut line = format!(
        "backdrop order={} bounds={} filters={}",
        backdrop.order,
        rect([
            backdrop.bounds.origin.x.0,
            backdrop.bounds.origin.y.0,
            backdrop.bounds.size.width.0,
            backdrop.bounds.size.height.0,
        ]),
        filters(&backdrop.filters)
    );
    if !backdrop.reads_only_what_it_writes() {
        line.push_str(&format!(
            " source={}",
            rect([
                backdrop.source.origin.x.0,
                backdrop.source.origin.y.0,
                backdrop.source.size.width.0,
                backdrop.source.size.height.0,
            ])
        ));
    }
    line.push_str(&suffix(scene, backdrop.clip, None));
    line
}

/// A filter chain.
pub fn filters(chain: &[Filter]) -> String {
    let rendered: Vec<String> = chain.iter().map(filter).collect();
    format!("[{}]", rendered.join(", "))
}

/// One filter function.
pub fn filter(filter: &Filter) -> String {
    match filter {
        Filter::Blur(deviation) => format!("blur({})", float(*deviation)),
        Filter::DropShadow {
            offset_x,
            offset_y,
            blur,
            color,
        } => format!(
            "drop_shadow({}, {}, {}, {})",
            float(*offset_x),
            float(*offset_y),
            float(*blur),
            paint::premultiplied(*color)
        ),
        Filter::Brightness(amount) => format!("brightness({})", float(*amount)),
        Filter::Contrast(amount) => format!("contrast({})", float(*amount)),
        Filter::Grayscale(amount) => format!("grayscale({})", float(*amount)),
        Filter::HueRotate(radians) => format!("hue_rotate({}rad)", float(*radians)),
        Filter::Invert(amount) => format!("invert({})", float(*amount)),
        Filter::Opacity(amount) => format!("opacity({})", float(*amount)),
        Filter::Saturate(amount) => format!("saturate({})", float(*amount)),
        Filter::Sepia(amount) => format!("sepia({})", float(*amount)),
    }
}
