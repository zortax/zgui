//! What a window remembers about a glyph it has already rasterised.
//!
//! # Why the atlas alone is not enough
//!
//! An [`AtlasTile`] says which texture holds a glyph's pixels and where in it they are. It does not
//! say where those pixels go on the screen: two glyphs of one extent share a tile shape and sit at
//! different heights above the baseline. The placement lives in the rasterised image, so a frame
//! holding the tile and not the placement has to rasterise the glyph again to learn where to put
//! it — which is the whole cost of rasterising, paid on every frame, for a cache hit.
//!
//! # Why absence is remembered too
//!
//! A space rasterises to an image with no pixels in it. Nothing is inserted into the atlas for one,
//! so a cache that only remembers tiles never remembers spaces, and every space on the page runs
//! the face's whole hinting program again on every full repaint. Absence is therefore a cached
//! answer here in its own right.
//!
//! # What is *not* remembered
//!
//! A key whose face could not be resolved, and a key the atlas had no room for. Both are states of
//! the world rather than properties of the glyph: a face registered later, or an eviction, makes
//! the same key succeed, and a remembered failure would outlive its cause.
//!
//! No room is reported rather than merely not remembered. A frame that draws nothing where a
//! letter belongs is recorded by the paint cache and replayed for as long as the fragment stands,
//! so "the atlas has room again next frame" is not on its own enough to bring the letter back —
//! the room has to be made while the letter is being placed.

use std::collections::VecDeque;

use rustc_hash::FxHashMap;
use zgui_atlas::{Atlas, AtlasKey, AtlasTile};
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_profile::{Counter, counter};
use zgui_text::{AtlasGlyph, GlyphKey, GlyphRaster};

use crate::content::vectors::VectorMaskCache;

/// A tile, what the atlas holds it under, and where the pixels in it sit relative to the glyph's
/// origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rasterised {
    /// Where the pixels are right now.
    pub(crate) tile: AtlasTile,
    /// What the atlas holds them under.
    ///
    /// The tile says which rectangle of which texture the pixels are in and nothing about what
    /// they are; the key is the name the rectangle can be reclaimed or held by, and it is the only
    /// one of the two a caller that wants to keep the pixels alive can use. A primitive carries
    /// the tile alone, which is why anything wanting to outlive a frame has to carry this as well.
    pub(crate) key: AtlasKey,
    /// The top-left corner of the pixels relative to the glyph's origin.
    pub(crate) placement: Point<DevicePx, Device>,
    /// How many pixels there are.
    pub(crate) size: Size<u32, Device>,
}

/// What a lookup answered.
///
/// Three outcomes rather than an option, because a caller can do something about exactly one of
/// them: nothing to draw is the final answer for a space and for a face with no outline, and no
/// room is a state of the atlas that freeing cold content changes.
enum Placed {
    /// The pixels, and where they are.
    Tile(Rasterised),
    /// There is nothing to draw.
    Nothing,
    /// There was no room. Freeing cold content and asking again may place it.
    NoRoom,
}

/// What one key rasterised to, last time anything asked.
#[derive(Clone, Copy, Debug)]
struct Remembered {
    /// What the atlas holds the pixels under, or `None` when there were no pixels.
    tile: Option<AtlasKey>,
    /// The top-left corner of the pixels relative to the glyph's origin.
    placement: Point<DevicePx, Device>,
    /// How many pixels there are.
    size: Size<u32, Device>,
}

/// Everything one window knows about the glyphs it has rasterised.
///
/// Lookups go through here rather than through the atlas, because the atlas answers a narrower
/// question than the one a frame is asking.
#[derive(Debug, Default)]
pub(crate) struct GlyphCache {
    /// Tile-backed entries plus a bounded set of blank-glyph answers.
    entries: FxHashMap<GlyphKey, Remembered>,
    /// The way back from an atlas eviction to the placement that named its tile.
    by_tile: FxHashMap<AtlasKey, GlyphKey>,
    /// Blank answers in insertion order, for bounding answers that own no atlas tile.
    blank_order: VecDeque<GlyphKey>,
    /// Recently evicted glyphs, so rebuilding them remains distinguishable from a first sighting.
    evicted: FxHashMap<GlyphKey, u64>,
    /// Tombstones in insertion order.
    evicted_order: VecDeque<(GlyphKey, u64)>,
    /// Monotonic identity for distinguishing a stale queue item from a newer tombstone.
    eviction_clock: u64,
}

/// Metadata with no atlas allocation is useful but must not grow with the life of the process.
const COLD_ANSWERS: usize = 4_096;

/// The things a frame writes to while it places glyphs.
///
/// Borrowed together because they are used together and are meaningless apart: a tile allocated in
/// the atlas without its entry in the cache is rasterised again next frame.
///
/// There is no device here and there cannot be one. Everything the atlas is asked for during a walk
/// — allocation, eviction bookkeeping, the queued texels, the queued texture — is arithmetic and
/// bytes; what a device is asked to do about it is decided afterwards, by whoever has one.
pub(crate) struct Rasterising<'a> {
    /// What each glyph rasterised to, last time anything asked.
    pub(crate) glyphs: &'a mut GlyphCache,
    /// The tiles.
    pub(crate) atlas: &'a mut Atlas,
    /// Geometry identities for small solid shapes stored in the monochrome atlas.
    pub(crate) vector_masks: &'a mut VectorMaskCache,
    /// Every atlas key handed out since the list was last drained.
    ///
    /// The list is how what a fragment *drew* becomes something that can be held: a primitive
    /// carries a rectangle of a texture and not the name the atlas knows it by, so once the walk
    /// has moved on there is no way back from the display list to the keys it depends on. Whoever
    /// is going to keep a range of that list has to be told the names while the placement is
    /// happening, and this is where they are collected.
    pub(crate) named: Vec<AtlasKey>,
}

impl Rasterising<'_> {
    /// The tile holding one glyph's pixels, rasterising it only if nothing cached can answer.
    pub(crate) fn tile_for(
        &mut self,
        raster: &dyn GlyphRaster,
        key: &GlyphKey,
    ) -> Option<Rasterised> {
        let Self {
            glyphs,
            atlas,
            vector_masks,
            named,
        } = self;
        let rasterised = match glyphs.tile_for(atlas, raster, key) {
            Placed::Tile(rasterised) => rasterised,
            Placed::Nothing => return None,
            // The pool is full of colder content. One eviction step spares everything this frame
            // has drawn and everything anything holds, so a single retry is safe — and it is
            // enough, because one step frees a whole generation.
            Placed::NoRoom => {
                let mut removed = Vec::new();
                let freed = atlas.evict_least_recently_used_into(&mut removed);
                glyphs.forget_tiles(&removed);
                vector_masks.forget_tiles(&removed);
                counter::add(Counter::AtlasTilesEvicted, freed.tiles as u64);
                match glyphs.tile_for(atlas, raster, key) {
                    Placed::Tile(rasterised) => rasterised,
                    Placed::Nothing | Placed::NoRoom => return None,
                }
            }
        };
        named.push(rasterised.key);
        Some(rasterised)
    }
}

impl GlyphCache {
    /// Forgets everything, which a lost device makes necessary.
    ///
    /// The atlas is emptied with it, so an entry kept here would name a tile that no longer exists
    /// and the placement it carries would be attached to whatever took that key's place.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.by_tile.clear();
        self.blank_order.clear();
        self.evicted.clear();
        self.evicted_order.clear();
        self.eviction_clock = 0;
    }

    /// How many keys have a remembered answer.
    pub(crate) fn held(&self) -> usize {
        self.entries.len()
    }

    /// The tile holding one glyph's pixels, rasterising it only if nothing here can answer.
    ///
    /// A glyph with no pixels and a face that could not be resolved both answer
    /// [`Placed::Nothing`]; an atlas with no room answers [`Placed::NoRoom`], which is a different
    /// thing entirely and is why the three are not one option.
    fn tile_for(&mut self, atlas: &mut Atlas, raster: &dyn GlyphRaster, key: &GlyphKey) -> Placed {
        if let Some(remembered) = self.entries.get(key).copied() {
            let Some(atlas_key) = remembered.tile else {
                // Known to rasterise to nothing. Rasterising it again would produce nothing again.
                return Placed::Nothing;
            };
            if let Some(tile) = atlas.get(atlas_key) {
                return Placed::Tile(Rasterised {
                    tile,
                    key: atlas_key,
                    placement: remembered.placement,
                    size: remembered.size,
                });
            }
            // The tile was evicted; the pixels have to be made again, and the entry replaced with
            // whatever the atlas gives them next. Counted separately from an ordinary miss,
            // because the two say opposite things: a first sighting of a glyph is the cache
            // working, and a second one is the budget taking back something still in use.
            counter::bump(Counter::RebuiltAfterEviction);
            self.entries.remove(key);
            self.by_tile.remove(&atlas_key);
        } else if self.forget_eviction(key) {
            counter::bump(Counter::RebuiltAfterEviction);
        }
        self.rasterise(atlas, raster, key)
    }

    /// Makes one glyph's pixels, uploads them, and remembers what happened.
    fn rasterise(&mut self, atlas: &mut Atlas, raster: &dyn GlyphRaster, key: &GlyphKey) -> Placed {
        crate::content::probe::rastered();
        // A face that has no bytes right now may have them later, so this is not remembered.
        let Some(image) = raster.raster(key) else {
            return Placed::Nothing;
        };
        if image.is_empty() || !image.is_well_formed() {
            self.entries.insert(
                *key,
                Remembered {
                    tile: None,
                    placement: image.placement,
                    size: image.size,
                },
            );
            self.blank_order.push_back(*key);
            self.prune_blanks();
            return Placed::Nothing;
        }
        let prepared = AtlasGlyph::of(key, &image);
        let extent = Size::new(prepared.size.width as i32, prepared.size.height as i32);
        // A full atlas is a state of this frame, not a property of the glyph, so a failure here
        // leaves nothing remembered and is reported as what it is.
        let Ok(tile) = atlas.get_or_insert(prepared.key, extent, || prepared.texels) else {
            return Placed::NoRoom;
        };
        if let Some(previous) = self.entries.insert(
            *key,
            Remembered {
                tile: Some(prepared.key),
                placement: image.placement,
                size: image.size,
            },
        ) && let Some(previous) = previous.tile
        {
            self.by_tile.remove(&previous);
        }
        self.by_tile.insert(prepared.key, *key);
        self.forget_eviction(key);
        Placed::Tile(Rasterised {
            tile,
            key: prepared.key,
            placement: image.placement,
            size: image.size,
        })
    }

    /// How many keys are remembered, whether or not they produced pixels.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Removes placement records for atlas tiles that have just been evicted.
    pub(crate) fn forget_tiles(&mut self, removed: &[AtlasKey]) {
        for atlas_key in removed {
            let Some(glyph) = self.by_tile.remove(atlas_key) else {
                continue;
            };
            if self
                .entries
                .get(&glyph)
                .is_some_and(|entry| entry.tile == Some(*atlas_key))
            {
                self.entries.remove(&glyph);
                self.remember_eviction(glyph);
            }
        }
    }

    fn prune_blanks(&mut self) {
        while self.blank_order.len() > COLD_ANSWERS {
            let Some(key) = self.blank_order.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&key)
                .is_some_and(|entry| entry.tile.is_none())
            {
                self.entries.remove(&key);
            }
        }
    }

    fn remember_eviction(&mut self, key: GlyphKey) {
        self.eviction_clock = self.eviction_clock.wrapping_add(1);
        let token = self.eviction_clock;
        self.evicted.insert(key, token);
        self.evicted_order.push_back((key, token));
        while self.evicted.len() > COLD_ANSWERS {
            let Some((oldest, token)) = self.evicted_order.pop_front() else {
                break;
            };
            if self.evicted.get(&oldest) == Some(&token) {
                self.evicted.remove(&oldest);
            }
        }
    }

    fn forget_eviction(&mut self, key: &GlyphKey) -> bool {
        self.evicted.remove(key).is_some()
    }
}

#[cfg(test)]
mod tests;
