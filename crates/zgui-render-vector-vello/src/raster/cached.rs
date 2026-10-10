//! One encoded sub-scene per distinct vector content, kept across frames.

use rustc_hash::FxHashMap;
use vello::Scene;
use zgui_scene::{PaintTable, VectorItem};

/// What an item's encoding depends on.
///
/// Anything that changes it is a different encoding; anything that does not — where the item is
/// placed this frame, which pass it landed in, what it is clipped by, which item it is — does not,
/// because re-placing a cached encoding is a copy and re-encoding it is a re-flattening. Two items
/// that agree on all of this share one encoding.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Fingerprint {
    /// The geometry, by the identity of the shared path rather than by its contents.
    path: usize,
    /// What fills it, and what strokes it, by content.
    paint: u64,
    /// The fill rule and the stroke style, packed.
    style: u64,
    /// The six coefficients of the item's brush, as bits.
    brush: [u64; 6],
}

/// One encoding, with the geometry it was made from.
struct Entry {
    /// The geometry, held so that its address cannot come to name a different path.
    #[expect(dead_code, reason = "held for its address, never read")]
    path: std::sync::Arc<kurbo::BezPath>,
    /// The encoded sub-scene.
    scene: Scene,
    /// The bytes the encoding holds.
    bytes: usize,
    /// The last frame that drew it.
    used: u64,
}

/// Every distinct content's encoded form, by what the encoding depends on.
///
/// This lives here rather than beside the display list for the same reason the whole crate does:
/// the display list may not name a rasteriser, and an encoded scene is a rasteriser's own form of
/// something the list holds backend-neutrally.
pub struct Encodings {
    /// The cached encodings, each holding the geometry it was encoded from.
    ///
    /// The path is kept alive rather than merely pointed at. A fingerprint identifies geometry by
    /// the address of its allocation, and an address only names one thing for as long as that thing
    /// exists: once the last owner outside this cache drops a path, the allocator is free to put a
    /// different path at the same address, and a fingerprint taken against it would compare equal to
    /// this entry's and hand back an encoding of the shape that used to be there. Holding the
    /// `Arc` makes the address the cache compares against one no live allocation can repeat.
    entries: FxHashMap<Fingerprint, Entry>,
    /// The bytes every held encoding holds together.
    bytes: usize,
    /// The bytes held past which encodings no frame drew recently are dropped.
    cap: usize,
    /// The frame lookups are counted against.
    frame: u64,
    /// How many lookups found what they wanted.
    hits: u64,
    /// How many had to encode.
    misses: u64,
}

impl Default for Encodings {
    fn default() -> Self {
        Self::with_cap(Self::BYTE_CAP)
    }
}

impl std::fmt::Debug for Encodings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Encodings")
            .field("entries", &self.entries.len())
            .field("bytes", &self.bytes)
            .field("hits", &self.hits)
            .field("misses", &self.misses)
            .finish()
    }
}

impl Encodings {
    /// The bytes held past which [`Encodings::end_frame`] drops encodings, least recently drawn
    /// first.
    ///
    /// A cache with no bound is a leak with a nice name: a document that scrolls through a thousand
    /// distinct icons would keep every one of them encoded for the life of the process.
    pub const BYTE_CAP: usize = 64 << 20;

    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty cache that holds `cap` bytes before it drops anything.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            entries: FxHashMap::default(),
            bytes: 0,
            cap,
            frame: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// The encoding of `item`, producing it with `encode` if there is not already one.
    pub fn get(
        &mut self,
        item: &VectorItem,
        paints: &PaintTable,
        encode: impl FnOnce(&mut Scene),
    ) -> &Scene {
        let wanted = fingerprint(item, paints);
        let frame = self.frame;
        let entry = match self.entries.entry(wanted) {
            std::collections::hash_map::Entry::Occupied(held) => {
                self.hits += 1;
                held.into_mut()
            }
            std::collections::hash_map::Entry::Vacant(free) => {
                self.misses += 1;
                let mut scene = Scene::new();
                encode(&mut scene);
                let bytes = bytes_of(&scene);
                self.bytes += bytes;
                free.insert(Entry {
                    path: std::sync::Arc::clone(&item.path),
                    scene,
                    bytes,
                    used: frame,
                })
            }
        };
        entry.used = frame;
        &entry.scene
    }

    /// Ends a frame: past the byte cap, drops the encodings no earlier frame drew most recently.
    ///
    /// Only past the cap, because the point of the cache is exactly that content nothing drew
    /// *this* frame is still there next frame; evicting every frame would make it a cache of one
    /// frame and re-encode a scrolled-away row the moment it came back. An encoding this frame drew
    /// stays even when the cap is exceeded, because the next frame draws it again.
    pub fn end_frame(&mut self) {
        if self.bytes > self.cap {
            let mut stale: Vec<(u64, Fingerprint)> = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.used < self.frame)
                .map(|(key, entry)| (entry.used, *key))
                .collect();
            stale.sort_unstable_by_key(|(used, _)| *used);
            for (_, key) in stale {
                if self.bytes <= self.cap {
                    break;
                }
                if let Some(entry) = self.entries.remove(&key) {
                    self.bytes -= entry.bytes;
                }
            }
        }
        self.frame += 1;
    }

    /// How many encodings are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The bytes every held encoding holds together.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// How many lookups re-used an encoding, and how many had to make one.
    pub fn counts(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}

/// The bytes one encoded sub-scene holds in its streams and its ramps.
fn bytes_of(scene: &Scene) -> usize {
    use std::mem::size_of_val;
    let encoding = scene.encoding();
    size_of_val(encoding.path_tags.as_slice())
        + size_of_val(encoding.path_data.as_slice())
        + size_of_val(encoding.draw_tags.as_slice())
        + size_of_val(encoding.draw_data.as_slice())
        + size_of_val(encoding.transforms.as_slice())
        + size_of_val(encoding.styles.as_slice())
        + size_of_val(encoding.resources.color_stops.as_slice())
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
        brush: item.brush.as_coeffs().map(f64::to_bits),
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

    /// Encodes one square into `into`, so the encoding holds bytes.
    fn encode(into: &mut vello::Scene) {
        into.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::WHITE,
            None,
            &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0),
        );
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

    #[test]
    fn equal_content_shares_one_encoding_across_identities() {
        let mut encodings = Encodings::new();
        let paints = zgui_scene::PaintTable::default();
        let path = square(32.0);

        encodings.get(&item(1, &path), &paints, encode);
        let mut encoded = false;
        encodings.get(&item(2, &path), &paints, |_| encoded = true);

        assert!(!encoded, "a second item with the same content was encoded");
        assert_eq!(encodings.len(), 1);
        assert_eq!(encodings.counts(), (1, 1));
    }

    #[test]
    fn a_different_brush_is_a_different_encoding() {
        let mut encodings = Encodings::new();
        let paints = zgui_scene::PaintTable::default();
        let path = square(32.0);

        encodings.get(&item(1, &path), &paints, encode);
        let brushed = item(1, &path).brushed(kurbo::Affine::translate((4.0, 0.0)));
        let mut encoded = false;
        encodings.get(&brushed, &paints, |_| encoded = true);

        assert!(
            encoded,
            "a moved brush is a different picture of the same path"
        );
        assert_eq!(encodings.len(), 2);
    }

    #[test]
    fn entries_past_the_byte_cap_go_oldest_first() {
        let paints = zgui_scene::PaintTable::default();
        let paths = [square(8.0), square(16.0), square(24.0)];
        let mut measure = Encodings::new();
        measure.get(&item(0, &paths[0]), &paints, encode);
        let one = measure.bytes();
        assert!(one > 0, "an encoding of a square holds bytes");

        // Room for two of the three.
        let mut encodings = Encodings::with_cap(2 * one);
        for path in &paths {
            encodings.get(&item(0, path), &paints, encode);
            encodings.end_frame();
        }
        assert_eq!(encodings.len(), 2);
        assert_eq!(encodings.bytes(), 2 * one);

        let mut encoded = false;
        encodings.get(&item(0, &paths[1]), &paints, |_| encoded = true);
        assert!(
            !encoded,
            "the second square was drawn after the first and stays"
        );
        encodings.get(&item(0, &paths[0]), &paints, |_| encoded = true);
        assert!(
            encoded,
            "the first square was drawn longest ago and went first"
        );
    }

    #[test]
    fn an_entry_used_this_frame_is_never_evicted() {
        let paints = zgui_scene::PaintTable::default();
        let paths = [square(8.0), square(16.0)];
        let mut encodings = Encodings::with_cap(1);
        for path in &paths {
            encodings.get(&item(0, path), &paints, encode);
        }
        encodings.end_frame();
        assert_eq!(
            encodings.len(),
            2,
            "a frame that drew both keeps both, over the cap or not"
        );

        // The next frame draws one of them, so the other is the one that goes.
        encodings.get(&item(0, &paths[1]), &paints, encode);
        encodings.end_frame();
        assert_eq!(encodings.len(), 1);
        let mut encoded = false;
        encodings.get(&item(0, &paths[1]), &paints, |_| encoded = true);
        assert!(!encoded);
    }
}
