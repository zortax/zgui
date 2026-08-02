//! One encoded sub-scene per vector item, kept across frames.

use std::collections::HashMap;

use vello::Scene;
use zgui_scene::{PaintTable, VectorId, VectorItem};

/// What an item's encoding depends on.
///
/// Anything that changes it invalidates the cached encoding; anything that does not — where the item
/// is placed this frame, which pass it landed in, what it is clipped by — deliberately does not,
/// because re-placing a cached encoding is a copy and re-encoding it is a re-flattening.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    /// The geometry, by the identity of the shared path rather than by its contents.
    path: usize,
    /// What fills it, and what strokes it, by content.
    paint: u64,
    /// The fill rule and the stroke width, packed.
    style: u64,
}

/// One item's encoding, with everything the encoding was made from.
struct Entry {
    /// What the encoding depends on, as it stood when the encoding was made.
    fingerprint: Fingerprint,
    /// The geometry, held so that its address cannot come to name a different path.
    path: std::sync::Arc<kurbo::BezPath>,
    /// The encoded sub-scene.
    scene: Scene,
}

/// Every item's encoded form, by the identity the display list gave it.
///
/// This lives here rather than beside the display list for the same reason the whole crate does:
/// the display list may not name a rasteriser, and an encoded scene is a rasteriser's own form of
/// something the list holds backend-neutrally.
#[derive(Default)]
pub struct Encodings {
    /// The cached encodings, each holding the geometry it was encoded from.
    ///
    /// The path is kept alive rather than merely pointed at. A fingerprint identifies geometry by
    /// the address of its allocation, and an address only names one thing for as long as that thing
    /// exists: once the last owner outside this cache drops a path, the allocator is free to put a
    /// different path at the same address, and a fingerprint taken against it would compare equal to
    /// this entry's and hand back an encoding of the shape that used to be there. Holding the
    /// `Arc` makes the address the cache compares against one no live allocation can repeat.
    entries: HashMap<VectorId, Entry>,
    /// How many frames' lookups found what they wanted.
    hits: u64,
    /// How many had to encode.
    misses: u64,
}

impl std::fmt::Debug for Encodings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Encodings")
            .field("entries", &self.entries.len())
            .field("hits", &self.hits)
            .field("misses", &self.misses)
            .finish()
    }
}

impl Encodings {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// The encoding of `item`, producing it with `encode` if there is not already a current one.
    pub fn get(
        &mut self,
        item: &VectorItem,
        paints: &PaintTable,
        encode: impl FnOnce(&mut Scene),
    ) -> &Scene {
        let wanted = fingerprint(item, paints);
        if self.entries.get(&item.id).map(|held| held.fingerprint) == Some(wanted) {
            self.hits += 1;
        } else {
            self.misses += 1;
            let entry = self.entries.entry(item.id).or_insert_with(|| Entry {
                fingerprint: wanted,
                path: std::sync::Arc::clone(&item.path),
                scene: Scene::new(),
            });
            entry.fingerprint = wanted;
            entry.path = std::sync::Arc::clone(&item.path);
            // Resetting is not optional: encoding into a scene that was not cleared grows it
            // without bound, one copy of the same path per frame.
            entry.scene.reset();
            encode(&mut entry.scene);
        }
        &self.entries[&item.id].scene
    }

    /// How many encodings may be held before the ones nothing drew are dropped.
    ///
    /// A cache with no bound is a leak with a nice name: a document that scrolls through a thousand
    /// distinct icons would keep every one of them encoded for the life of the process.
    pub const CAPACITY: usize = 512;

    /// Forgets everything not in `live`, once there is more held than [`Encodings::CAPACITY`].
    ///
    /// Only once over the bound, because the point of the cache is exactly that an icon nothing drew
    /// *this* frame is still there next frame; evicting every frame would make it a cache of one
    /// frame and re-encode a scrolled-away row the moment it came back.
    pub fn retain_if_over_capacity(&mut self, live: impl Fn(VectorId) -> bool) {
        if self.entries.len() <= Self::CAPACITY {
            return;
        }
        self.entries.retain(|id, _| live(*id));
    }

    /// How many encodings are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many lookups re-used an encoding, and how many had to make one.
    pub fn counts(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}

/// What `item`'s encoding depends on.
fn fingerprint(item: &VectorItem, paints: &PaintTable) -> Fingerprint {
    let paint_hash = |reference: Option<zgui_scene::PaintRef>| match reference.and_then(|r| r.id())
    {
        Some(id) => paints.content_hash(id).unwrap_or(0),
        None => 0,
    };
    Fingerprint {
        path: std::sync::Arc::as_ptr(&item.path) as usize,
        paint: paint_hash(item.fill)
            .wrapping_mul(31)
            .wrapping_add(paint_hash(item.stroke.as_ref().map(|stroke| stroke.paint))),
        style: u64::from(item.fill_rule as u32)
            .wrapping_mul(1_000_003)
            .wrapping_add(stroke_hash(
                item.stroke.as_ref().map(|stroke| &stroke.style),
            )),
    }
}

/// Every number a stroke's outline depends on, folded together.
///
/// The width alone is not enough and the difference is visible: a dashed line re-styled solid, or a
/// mitred corner re-styled round, moves no geometry and changes no paint, so a fingerprint that
/// looked only at the width would hand back the previous encoding and keep drawing the old shape.
fn stroke_hash(stroke: Option<&kurbo::Stroke>) -> u64 {
    let Some(stroke) = stroke else {
        return 0;
    };
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut fold = |value: u64| {
        hash ^= value;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    };
    fold(stroke.width.to_bits());
    fold(stroke.miter_limit.to_bits());
    fold(stroke.dash_offset.to_bits());
    fold(stroke.join as u64);
    fold(stroke.start_cap as u64);
    fold(stroke.end_cap as u64);
    for dash in &stroke.dash_pattern {
        fold(dash.to_bits());
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_scene::{VectorId, VectorItem};

    use super::Encodings;

    /// A filled path around one square, with no paint to look up.
    fn item(id: u32, path: &Arc<kurbo::BezPath>) -> VectorItem {
        let mut item = VectorItem::filled(
            VectorId(id),
            Arc::clone(path),
            zgui_scene::PaintRef::solid(zgui_scene::PaintId(0)),
        );
        item.fill = None;
        item
    }

    /// One square, as geometry.
    fn square(size: f64) -> Arc<kurbo::BezPath> {
        use kurbo::Shape;
        Arc::new(kurbo::Rect::new(0.0, 0.0, size, size).to_path(0.1))
    }

    /// A cached encoding keeps the geometry it was made from alive.
    ///
    /// The cache decides whether an encoding is still current by comparing the address of the
    /// path's allocation. An address names one allocation only while that allocation exists, so an
    /// entry that merely recorded the address would compare equal to any later path the allocator
    /// happened to place there, and hand back an encoding of a shape that is no longer drawn.
    /// Holding the path is what makes the comparison sound, and it is a property of the cache
    /// rather than of the allocator, so it is asserted directly.
    #[test]
    fn the_geometry_an_encoding_was_made_from_is_held_by_the_cache() {
        let mut encodings = Encodings::new();
        let path = square(32.0);
        let paints = zgui_scene::PaintTable::default();

        encodings.get(&item(1, &path), &paints, |_| {});

        assert!(
            Arc::strong_count(&path) > 1,
            "the cache let go of the geometry it identifies its entry by, so the allocator is free \
             to put a different path at that address and the entry would match it"
        );
    }

    /// Geometry with different contents at a repeated address is encoded again, not re-used.
    ///
    /// This is the failure the holding prevents, staged as directly as it can be: the first path is
    /// dropped by everything except the cache, a second is built, and the cache is asked for the
    /// same identity. Whether the allocator repeats the address is its own business — the assertion
    /// is that the answer does not depend on it.
    #[test]
    fn a_replaced_path_is_encoded_again_however_the_allocator_places_it() {
        let mut encodings = Encodings::new();
        let paints = zgui_scene::PaintTable::default();

        let first = square(32.0);
        encodings.get(&item(1, &first), &paints, |_| {});
        drop(first);

        let second = square(64.0);
        let mut encoded = false;
        encodings.get(&item(1, &second), &paints, |_| encoded = true);

        assert!(
            encoded,
            "the cache handed back the encoding of the path that used to be there"
        );
    }
}
