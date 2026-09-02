//! Clip chains, resolved through the scene's table rather than printed as indices.
//!
//! A chain is rendered as its **links**, innermost last, and not as the flattened rectangle a draw
//! call binds. The flattening deliberately drops rounded tests beyond the two one draw call can
//! apply, so a transcript built from it would render two chains that clip differently as the same
//! text — which is exactly the case
//! [`ClipTable::needs_group_target`](zgui_scene::ClipTable::needs_group_target) exists to catch,
//! and the case a golden most needs to see.

use zgui_scene::{ClipId, ClipLink, ClipTable, MaskSource};

use crate::text::number::{all_zero, list, rect};
use crate::transcript::tile;

/// A clip chain, by its links.
///
/// [`ClipId::ROOT`] renders as `root`, which is what the overwhelming majority of primitives carry.
pub fn chain(table: &ClipTable, id: ClipId) -> String {
    if id.is_root() {
        return "root".to_owned();
    }
    let links = table.links(id);
    if links.is_empty() {
        return format!("#{} <missing>", id.index());
    }
    let rendered: Vec<String> = links.iter().map(link).collect();
    let promoted = if table.needs_group_target(id) {
        " needs_group_target"
    } else {
        ""
    };
    format!("[{}]{promoted}", rendered.join(" > "))
}

/// One link of a chain.
pub fn link(link: &ClipLink) -> String {
    match link {
        ClipLink::RoundedRect {
            rect: bounds,
            radii,
            shape,
            ..
        } => {
            let corners = [
                radii.top_left.x.0,
                radii.top_left.y.0,
                radii.top_right.x.0,
                radii.top_right.y.0,
                radii.bottom_right.x.0,
                radii.bottom_right.y.0,
                radii.bottom_left.x.0,
                radii.bottom_left.y.0,
            ];
            let bounds = rect([
                bounds.origin.x.0,
                bounds.origin.y.0,
                bounds.size.width.0,
                bounds.size.height.0,
            ]);
            // The shape is printed only when it is not the ellipse, so every transcript written
            // before corner shapes existed still reads the way it did.
            let cut = if shape.is_round() {
                String::new()
            } else {
                format!(" shape={}", crate::text::number::float(shape.get()))
            };
            if all_zero(&corners) {
                format!("{bounds}{cut}")
            } else {
                format!("{bounds} radii={}{cut}", list(&corners))
            }
        }
        ClipLink::Mask {
            tile: atlas,
            transform,
            source,
        } => format!(
            "mask {} transform=#{} source={}",
            tile::of(*atlas),
            transform.index(),
            match source {
                MaskSource::Path => "path",
                MaskSource::Raster => "raster",
            }
        ),
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Corners, DevicePx, Point, Rect, Size, Vec2};
    use zgui_scene::{ClipId, ClipLink, ClipTable};

    use super::chain;

    /// A device rectangle.
    fn bounds(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, zgui_geom::Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn the_chain_that_clips_nothing_has_a_name() {
        let table = ClipTable::rooted();
        assert_eq!(chain(&table, ClipId::ROOT), "root");
    }

    #[test]
    fn a_chain_renders_every_link_outermost_first() {
        let mut table = ClipTable::rooted();
        let outer = table.only(ClipLink::rect(bounds(0.0, 0.0, 100.0, 100.0)));
        let inner = table.push(outer, ClipLink::rect(bounds(0.0, 0.0, 50.0, 50.0)));
        assert_eq!(
            chain(&table, inner),
            "[rect(0, 0, 100, 100) > rect(0, 0, 50, 50)]"
        );
    }

    #[test]
    fn a_third_rounded_link_is_visible_even_though_flattening_drops_it() {
        // Flattening keeps two rounded tests. A transcript built from the flattened form would
        // render this chain identically to its two-link prefix, and the promotion to a group target
        // — a real, visible difference — would never appear in a golden.
        let mut table = ClipTable::rooted();
        let radius = Vec2::splat(DevicePx(4.0));
        let mut id = ClipId::ROOT;
        for step in 0..3 {
            id = table.push(
                id,
                ClipLink::RoundedRect {
                    shape: zgui_scene::CornerShape::ROUND,
                    rect: bounds(0.0, 0.0, 100.0 - step as f32, 100.0),
                    radii: Corners::uniform(radius),
                    space: zgui_scene::SpatialId::VIEWPORT,
                },
            );
        }
        let rendered = chain(&table, id);
        assert_eq!(rendered.matches("radii=").count(), 3);
        assert!(rendered.ends_with("needs_group_target"));
    }
}
