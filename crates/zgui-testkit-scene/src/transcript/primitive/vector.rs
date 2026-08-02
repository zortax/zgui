//! Vector content, and content the renderer did not draw.

use zgui_scene::{ExternalQuad, Scene, VectorItem};

use crate::text::number::{float, rect};
use crate::transcript::primitive::suffix;
use crate::transcript::{paint, path};

/// One filled or stroked path.
///
/// The path data is printed, not only its bounding box: two different paths with the same box draw
/// differently, and a transcript that showed only the box would hold a golden green through a
/// geometry regression.
pub fn vector(scene: &Scene, item: &VectorItem) -> String {
    let mut line = format!(
        "vector order={} id=#{} ink={}",
        item.order,
        item.id.index(),
        rect([
            item.ink.origin.x.0,
            item.ink.origin.y.0,
            item.ink.size.width.0,
            item.ink.size.height.0,
        ])
    );
    if let Some(fill) = item.fill {
        line.push_str(&format!(
            " fill={} fill_rule={:?}",
            paint::reference(&scene.paints, fill),
            item.fill_rule
        ));
    }
    if let Some(stroke) = item.stroke.as_ref() {
        line.push_str(&format!(
            " stroke={} width={}",
            paint::reference(&scene.paints, stroke.paint),
            float(stroke.width())
        ));
        // Only what departs from a plain stroke is printed, so that every transcript already
        // written stays the transcript it was and a dash pattern nobody asked for cannot appear.
        let plain = zgui_scene::kurbo::Stroke::new(stroke.style.width);
        if stroke.style.start_cap != plain.start_cap || stroke.style.end_cap != plain.end_cap {
            line.push_str(&format!(
                " caps={:?}/{:?}",
                stroke.style.start_cap, stroke.style.end_cap
            ));
        }
        if stroke.style.join != plain.join {
            line.push_str(&format!(" join={:?}", stroke.style.join));
        }
        if stroke.style.miter_limit != plain.miter_limit {
            line.push_str(&format!(
                " miter={}",
                float(stroke.style.miter_limit as f32)
            ));
        }
        if !stroke.style.dash_pattern.is_empty() {
            let dashes: Vec<String> = stroke
                .style
                .dash_pattern
                .iter()
                .map(|dash| float(*dash as f32))
                .collect();
            line.push_str(&format!(
                " dashes=[{}]@{}",
                dashes.join(","),
                float(stroke.style.dash_offset as f32)
            ));
        }
    }
    for clip in &item.clips {
        line.push_str(&format!(
            " inside=\"{}\" clip_rule={:?}",
            path::of(&clip.path),
            clip.rule
        ));
    }
    line.push_str(&format!(" d=\"{}\"", path::of(&item.path)));
    line.push_str(&suffix(scene, item.clip, item.transform));
    line
}

/// A texture the renderer did not draw.
pub fn external(scene: &Scene, external: &ExternalQuad) -> String {
    let mut line = format!(
        "external order={} bounds={} texture=#{}",
        external.order,
        rect([
            external.bounds.origin.x.0,
            external.bounds.origin.y.0,
            external.bounds.size.width.0,
            external.bounds.size.height.0,
        ]),
        external.texture.0
    );
    if external.opacity != 1.0 {
        line.push_str(&format!(" opacity={}", float(external.opacity)));
    }
    line.push_str(&suffix(scene, external.clip, Some(external.transform)));
    line
}
