//! What happens to a sprite whose resource was named and then placed, and to one that was not.

use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::prim::{ColorSprite, MonoSprite};
use crate::resource::{ResourceGeneration, ResourceKey, ResourceRegistry};
use crate::scene::Scene;

/// A rectangle on the surface.
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(w), DevicePx(h)),
    )
}

/// A tile of the mono pool, eight by sixteen at the given origin.
fn tile(x: i32) -> AtlasTile {
    AtlasTile {
        texture: TextureId::new(TextureKind::Mono, 0),
        tile: TileId(7),
        bounds: Rect::new(Point::new(x, 0), Size::new(8, 16)),
    }
}

/// The name of one glyph's pixels in the first generation.
fn name(hash: u64) -> ResourceKey {
    ResourceKey::new(TextureKind::Mono, hash, ResourceGeneration::FIRST)
}

/// A scene with one viewport-sized frame started.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));
    scene
}

#[test]
fn a_sprite_pushed_with_a_name_carries_it_until_the_registry_is_consulted() {
    let mut scene = scene();
    scene.push_mono_sprite(MonoSprite::new(
        rect(0.0, 0.0, 8.0, 16.0),
        name(1),
        Color::WHITE,
    ));

    let pushed = scene.primitives.mono_sprites[0];
    assert!(pushed.tile.is_unresolved(), "nothing has placed it yet");
    assert_eq!(
        pushed.tile.key(),
        Some(name(1)),
        "and the name survives the round trip through the instance's own words"
    );
    assert!(scene.has_unresolved_resources());

    let mut registry = ResourceRegistry::new();
    registry.place(name(1), tile(32));
    assert_eq!(scene.resolve_resources(&registry), 1);

    assert!(!scene.has_unresolved_resources());
    assert_eq!(
        scene.primitives.mono_sprites[0].tile,
        crate::prim::SpriteTile::of(tile(32)),
        "the fix-up leaves exactly what pushing the placement would have left"
    );
}

#[test]
fn a_sprite_pushed_with_a_placement_is_never_waiting_for_one() {
    let mut scene = scene();
    scene.push_mono_sprite(MonoSprite::new(
        rect(0.0, 0.0, 8.0, 16.0),
        tile(0),
        Color::WHITE,
    ));
    assert!(
        !scene.has_unresolved_resources(),
        "the ordinary path costs the fix-up nothing to do"
    );
    assert_eq!(scene.resolve_resources(&ResourceRegistry::new()), 0);
}

#[test]
fn a_name_from_a_discarded_generation_resolves_to_nothing() {
    let mut scene = scene();
    scene.push_color_sprite(ColorSprite::new(rect(0.0, 0.0, 8.0, 8.0), name(9)));

    let mut registry = ResourceRegistry::new();
    registry.place(name(9), tile(0));
    // Everything cached is thrown away, which is what a lost device does. The name the sprite
    // carries pointed into a texture that no longer exists.
    registry.discard();
    registry.place(
        ResourceKey::new(TextureKind::Mono, 9, registry.generation()),
        tile(64),
    );

    assert_eq!(
        scene.resolve_resources(&registry),
        0,
        "a stale name is refused rather than resolved to whatever took its place"
    );
    assert!(scene.has_unresolved_resources());
}

/// A sprite that never got a placement stops a debug build rather than reaching a device.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "naming a resource nothing placed")]
fn an_unresolved_sprite_never_reaches_a_draw() {
    let mut scene = scene();
    scene.push_mono_sprite(MonoSprite::new(
        rect(0.0, 0.0, 8.0, 16.0),
        name(1),
        Color::WHITE,
    ));
    // The registry is deliberately withholding the resolution, which is the whole fixture.
    scene.resolve_resources(&ResourceRegistry::new());
    scene.finish(&DamageSet::full());
}

/// A sprite that never got a placement draws nothing and forces its range to be emitted again.
#[cfg(not(debug_assertions))]
#[test]
fn an_unresolved_sprite_never_reaches_a_draw() {
    let mut scene = scene();
    scene.push_mono_sprite(MonoSprite::new(
        rect(0.0, 0.0, 8.0, 16.0),
        name(1),
        Color::WHITE,
    ));
    scene.resolve_resources(&ResourceRegistry::new());

    let before = scene.unreplayable();
    scene.finish(&DamageSet::full());

    let sprite = scene.primitives.mono_sprites[0];
    assert!(
        !sprite.tile.is_unresolved(),
        "a placeholder would have sampled texel zero of texture zero, which is another glyph"
    );
    assert_eq!(sprite.bounds, [0.0; 4], "and it covers nothing");
    assert!(
        scene.unreplayable() > before,
        "the range it was in cannot stand in for the drawing next frame"
    );
}
