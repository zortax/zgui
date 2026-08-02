//! The three sprite primitives.
//!
//! The two coverage kinds are laid out identically and differ only in the pipeline that draws them,
//! so they share a rendering and differ only in the name it carries. Writing the body twice would
//! be the more direct way to have one of them silently stop printing a field.

use zgui_scene::{ClipId, ColorSprite, MonoSprite, Scene, SpriteTile, SubpixelSprite};

use crate::text::number::{all_zero, float, list, rect};
use crate::transcript::primitive::suffix;
use crate::transcript::{paint, tile};

/// The fields the two coverage sprites share.
struct Coverage {
    /// Where it draws in the painting order.
    order: u32,
    /// Where it lands on the surface.
    bounds: [f32; 4],
    /// The tint the coverage is multiplied by.
    color: [f32; 4],
    /// The coverage tile.
    tile: SpriteTile,
    /// The chain it draws through.
    clip: u32,
    /// The transform it draws under.
    transform: u32,
}

impl From<&MonoSprite> for Coverage {
    fn from(sprite: &MonoSprite) -> Self {
        Self {
            order: sprite.order,
            bounds: sprite.bounds,
            color: sprite.color,
            tile: sprite.tile,
            clip: sprite.clip,
            transform: sprite.transform,
        }
    }
}

impl From<&SubpixelSprite> for Coverage {
    fn from(sprite: &SubpixelSprite) -> Self {
        Self {
            order: sprite.order,
            bounds: sprite.bounds,
            color: sprite.color,
            tile: sprite.tile,
            clip: sprite.clip,
            transform: sprite.transform,
        }
    }
}

/// A single-channel coverage sprite.
pub fn mono_sprite(scene: &Scene, sprite: &MonoSprite) -> String {
    coverage(scene, "mono_sprite", &Coverage::from(sprite))
}

/// A three-channel coverage sprite.
pub fn subpixel_sprite(scene: &Scene, sprite: &SubpixelSprite) -> String {
    coverage(scene, "subpixel_sprite", &Coverage::from(sprite))
}

/// A full-colour sprite.
pub fn color_sprite(scene: &Scene, sprite: &ColorSprite) -> String {
    let mut line = format!(
        "color_sprite order={} bounds={} tile={}",
        sprite.order,
        rect(sprite.bounds),
        tile::packed(sprite.tile)
    );
    if sprite.opacity != 1.0 {
        line.push_str(&format!(" opacity={}", float(sprite.opacity)));
    }
    if sprite.flags & ColorSprite::GRAYSCALE != 0 {
        line.push_str(" grayscale");
    }
    if !all_zero(&sprite.radii) {
        line.push_str(&format!(" radii={}", list(&sprite.radii)));
    }
    line.push_str(&suffix(
        scene,
        ClipId(sprite.clip),
        scene.spatial.at(sprite.transform),
    ));
    line
}

/// The shared body of the two coverage kinds.
fn coverage(scene: &Scene, name: &str, sprite: &Coverage) -> String {
    let mut line = format!(
        "{name} order={} bounds={} color={} tile={}",
        sprite.order,
        rect(sprite.bounds),
        paint::premultiplied(sprite.color),
        tile::packed(sprite.tile)
    );
    line.push_str(&suffix(
        scene,
        ClipId(sprite.clip),
        scene.spatial.at(sprite.transform),
    ));
    line
}
