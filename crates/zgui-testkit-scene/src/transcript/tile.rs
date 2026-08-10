//! Atlas tiles, as a transcript names them.

use zgui_atlas::{AtlasTile, TextureKind};
use zgui_scene::SpriteTile;

/// One atlas allocation: which pool, which texture of it, which tile, and where in the texture.
pub fn of(tile: AtlasTile) -> String {
    format!(
        "{}:{}#{} texels=rect({}, {}, {}, {})",
        pool(tile.texture.kind),
        tile.texture.index,
        tile.tile.0,
        tile.bounds.origin.x,
        tile.bounds.origin.y,
        tile.bounds.size.width,
        tile.bounds.size.height
    )
}

/// The same, from the packed form an instance carries.
///
/// The pool and the texture index are packed into one word by the instance encoding, so they are
/// unpacked here rather than printed as the packed number — a reviewer reads "mono:0", not
/// "texture=65536".
pub fn packed(tile: SpriteTile) -> String {
    let kind = match tile.texture >> 16 {
        0 => "mono",
        1 => "subpixel",
        2 => "color",
        3 => "image",
        _ => "<unknown>",
    };
    format!(
        "{kind}:{}#{} texels=rect({}, {}, {}, {})",
        tile.texture & 0xffff,
        tile.tile,
        tile.bounds[0],
        tile.bounds[1],
        tile.bounds[2],
        tile.bounds[3]
    )
}

/// A texture pool's name.
pub fn pool(kind: TextureKind) -> &'static str {
    match kind {
        TextureKind::Mono => "mono",
        TextureKind::Subpixel => "subpixel",
        TextureKind::Color => "color",
        TextureKind::Image => "image",
    }
}

#[cfg(test)]
mod tests {
    use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
    use zgui_geom::{Point, Rect, Size};
    use zgui_scene::SpriteTile;

    use super::{of, packed};

    /// One allocation in the colour pool.
    fn tile() -> AtlasTile {
        AtlasTile {
            texture: TextureId::new(TextureKind::Color, 1),
            tile: TileId(4),
            bounds: Rect::new(Point::new(2, 3), Size::new(8, 16)),
        }
    }

    #[test]
    fn the_packed_and_unpacked_forms_render_identically() {
        // They describe the same allocation, so a transcript that rendered them differently would
        // diff a sprite against the image paint reading the very same tile.
        assert_eq!(of(tile()), packed(SpriteTile::of(tile())));
        assert_eq!(of(tile()), "color:1#4 texels=rect(2, 3, 8, 16)");
    }
}
