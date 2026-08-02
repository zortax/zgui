//! The fix-up: turning the names sprites were pushed with into the placements they draw from.

use zgui_profile::{Counter, counter};

use crate::prim::{PrimitiveKind, SpriteTile};
use crate::resource::ResourceRegistry;
use crate::scene::Scene;

/// Where a sprite that named a resource is waiting for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Unresolved {
    /// Which array it is in.
    pub(crate) kind: PrimitiveKind,
    /// Where in that array, before the frame is sorted.
    pub(crate) index: u32,
}

impl Scene {
    /// Notes that a sprite of `kind` at `index` carries a name rather than a placement.
    pub(crate) fn note_resource(&mut self, kind: PrimitiveKind, index: usize, tile: SpriteTile) {
        if !tile.is_unresolved() {
            counter::bump(Counter::SpritesResolvedAtPush);
            return;
        }
        self.unresolved.push(Unresolved {
            kind,
            index: index as u32,
        });
    }

    /// Whether any sprite pushed this frame is still waiting for a placement.
    pub fn has_unresolved_resources(&self) -> bool {
        !self.unresolved.is_empty()
    }

    /// Fills in the placement of every sprite that was pushed with a name, and reports how many.
    ///
    /// Runs over exactly the instances that need it, and has to run **before**
    /// [`Scene::finish`](Scene::finish): the arrays are sorted there, by a key whose first
    /// component after draw order is the texture, so a placeholder resolved afterwards would sit
    /// where the sprite it used to be sorted as, rather than beside the others sharing its texture.
    ///
    /// A name the registry cannot resolve is left alone rather than guessed at. What happens to it
    /// is [`Scene::finish`]'s decision, and there is nothing useful this can do about it.
    pub fn resolve_resources(&mut self, registry: &ResourceRegistry) -> usize {
        let mut fixed = 0;
        let mut waiting = core::mem::take(&mut self.unresolved);
        let mut still = Vec::new();
        for entry in waiting.drain(..) {
            let Some(slot) = self.tile_at(entry) else {
                continue;
            };
            match slot.key().and_then(|key| registry.tile(key)) {
                Some(tile) => {
                    *slot = SpriteTile::of(tile);
                    fixed += 1;
                }
                None => still.push(entry),
            }
        }
        self.unresolved = still;
        counter::add(Counter::SpritesFixedUp, fixed as u64);
        fixed
    }

    /// The tile word of the sprite one waiting entry names.
    fn tile_at(&mut self, entry: Unresolved) -> Option<&mut SpriteTile> {
        let index = entry.index as usize;
        match entry.kind {
            PrimitiveKind::MonoSprite => self
                .primitives
                .mono_sprites
                .get_mut(index)
                .map(|sprite| &mut sprite.tile),
            PrimitiveKind::SubpixelSprite => self
                .primitives
                .subpixel_sprites
                .get_mut(index)
                .map(|sprite| &mut sprite.tile),
            PrimitiveKind::ColorSprite => self
                .primitives
                .color_sprites
                .get_mut(index)
                .map(|sprite| &mut sprite.tile),
            _ => None,
        }
    }

    /// Refuses to let a sprite that never got a placement reach a device.
    ///
    /// A placeholder is not a blank: its texture word is out of range and its rectangle is the
    /// name's own bits, so what a shader would sample is texel zero of texture zero — another
    /// glyph's pixels, drawn where this one's were meant to go, with nothing anywhere reporting a
    /// fault. So a debug build stops, and a release build draws nothing for it and takes the range
    /// out of the replayable log, which makes the next frame emit it again from scratch.
    pub(crate) fn refuse_unresolved(&mut self) {
        debug_assert_eq!(
            self.stray_placeholders(),
            self.unresolved.len(),
            "a sprite carrying a name that the waiting list does not know about"
        );
        if self.unresolved.is_empty() {
            return;
        }
        debug_assert!(
            false,
            "{} sprites reached the end of the frame naming a resource nothing placed",
            self.unresolved.len()
        );
        for entry in core::mem::take(&mut self.unresolved) {
            let index = entry.index as usize;
            match entry.kind {
                PrimitiveKind::MonoSprite => {
                    let sprite = &mut self.primitives.mono_sprites[index];
                    blank(&mut sprite.tile, &mut sprite.bounds);
                }
                PrimitiveKind::SubpixelSprite => {
                    let sprite = &mut self.primitives.subpixel_sprites[index];
                    blank(&mut sprite.tile, &mut sprite.bounds);
                }
                PrimitiveKind::ColorSprite => {
                    let sprite = &mut self.primitives.color_sprites[index];
                    blank(&mut sprite.tile, &mut sprite.bounds);
                }
                _ => continue,
            }
            self.note_unreplayable();
        }
    }

    /// How many sprites of this frame carry a name, counted from the arrays themselves.
    ///
    /// The independent reading of what the waiting list claims. It exists because the list is built
    /// as sprites are pushed and a sprite can reach an array without being pushed — a replayed range
    /// is copied in wholesale — so a list that agreed with itself would say nothing about those.
    /// Three linear scans of one word each, and only where the assertions are compiled in.
    fn stray_placeholders(&self) -> usize {
        let mono = self
            .primitives
            .mono_sprites
            .iter()
            .filter(|sprite| sprite.tile.is_unresolved());
        let subpixel = self
            .primitives
            .subpixel_sprites
            .iter()
            .filter(|sprite| sprite.tile.is_unresolved())
            .count();
        let color = self
            .primitives
            .color_sprites
            .iter()
            .filter(|sprite| sprite.tile.is_unresolved())
            .count();
        mono.count() + subpixel + color
    }
}

/// Turns one sprite into something that samples nothing and covers nothing.
fn blank(tile: &mut SpriteTile, bounds: &mut [f32; 4]) {
    *tile = SpriteTile::default();
    *bounds = [0.0; 4];
}

#[cfg(test)]
mod tests;
