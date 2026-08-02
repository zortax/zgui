//! The whole policy, driven by the in-memory sink at CPU speed.

use proptest::prelude::*;
use zgui_geom::Size;

use crate::atlas::{Atlas, AtlasLimits};
use crate::error::AtlasError;
use crate::key::AtlasKey;
use crate::sink::{MemorySink, TextureSink};
use crate::texture::{TextureFormat, TextureId, TextureKind};

/// A key of the monochrome pool.
fn mono(handle: u64) -> AtlasKey {
    AtlasKey::new(handle, TextureKind::Mono)
}

/// A square tile size.
fn square(side: i32) -> Size<i32, zgui_geom::Device> {
    Size::new(side, side)
}

/// An atlas with room for a few thousand small tiles, and a sink to drive it.
fn atlas() -> (Atlas, MemorySink) {
    (Atlas::new(AtlasLimits::default()), MemorySink::new())
}

#[test]
fn a_miss_allocates_and_a_hit_does_not() {
    let (mut atlas, _sink) = atlas();
    let first = atlas
        .get_or_insert(mono(1), square(8), || vec![1; 64])
        .unwrap();
    let second = atlas
        .get_or_insert(mono(1), square(8), || panic!("build must not run on a hit"))
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(atlas.len(), 1);
    assert_eq!(atlas.report().pending_uploads, 1);
}

#[test]
fn uploads_are_deferred_until_they_are_flushed() {
    let (mut atlas, mut sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(4), || vec![9; 16])
        .unwrap();
    assert_eq!(sink.bytes_written(), 0);

    assert_eq!(atlas.flush_uploads(&mut sink).unwrap(), 16);
    assert_eq!(sink.writes(), 1);
    assert_eq!(atlas.report().pending_uploads, 0);
}

#[test]
fn removing_before_a_flush_discards_the_upload_that_would_land_in_someone_elses_tile() {
    let (mut atlas, mut sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(4), || vec![9; 16])
        .unwrap();
    assert!(atlas.remove(mono(1)));

    assert_eq!(atlas.report().pending_uploads, 0);
    assert_eq!(atlas.report().pending_bytes, 0);
    assert_eq!(atlas.flush_uploads(&mut sink).unwrap(), 0);
}

#[test]
fn the_bytes_land_where_the_tile_says_they_did() {
    let (mut atlas, mut sink) = atlas();
    let tile = atlas
        .get_or_insert(mono(1), square(2), || vec![0x5a; 4])
        .unwrap();
    atlas.flush_uploads(&mut sink).unwrap();

    let x = tile.bounds.origin.x;
    let y = tile.bounds.origin.y;
    assert_eq!(sink.texel(tile.texture, x, y), Some(&[0x5a][..]));
}

#[test]
fn a_tile_larger_than_any_texture_is_refused_rather_than_retried() {
    let (mut atlas, _sink) = atlas();
    let error = atlas
        .get_or_insert(mono(1), square(8192), Vec::new)
        .unwrap_err();
    assert!(matches!(error, AtlasError::TooLarge { .. }));
}

#[test]
fn a_build_that_disagrees_with_the_size_returns_the_space_it_was_given() {
    let (mut atlas, _sink) = atlas();
    let error = atlas
        .get_or_insert(mono(1), square(4), || vec![0; 3])
        .unwrap_err();
    assert!(matches!(error, AtlasError::WrongByteCount { .. }));
    assert!(atlas.is_fully_reclaimed());
    assert!(atlas.is_empty());
}

#[test]
fn the_three_pools_are_separate_and_keep_their_own_formats() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(4), || vec![0; 16])
        .unwrap();
    atlas
        .get_or_insert(AtlasKey::new(1, TextureKind::Color), square(4), || {
            vec![0; 64]
        })
        .unwrap();

    assert_eq!(atlas.len(), 2);
    assert_eq!(atlas.report().textures, 2);
    assert_eq!(
        TextureKind::Mono.format().bytes_per_texel(),
        TextureFormat::R8Unorm.bytes_per_texel()
    );
}

/// The reference implementation's bug, asserted absent: it never returns a tile's space, so a
/// session that churns glyph variants needs a fresh texture every few hundred of them and grows
/// without bound.
///
/// One long-lived tile is held throughout, so the texture is never destroyed and recreated and the
/// count below is a statement about reclaimed space rather than about texture churn: a hundred
/// thousand distinct tiles pass through **one** texture, and the space they occupied comes back.
#[test]
fn a_hundred_thousand_alloc_free_cycles_leak_no_tile_space() {
    let (mut atlas, mut sink) = atlas();
    let anchor = mono(u64::MAX);
    atlas
        .get_or_insert(anchor, square(16), || vec![0; 256])
        .unwrap();
    atlas.retain(anchor);

    for cycle in 0..100_000u64 {
        let key = mono(cycle);
        atlas
            .get_or_insert(key, square(16), || vec![0; 256])
            .unwrap_or_else(|error| panic!("cycle {cycle} could not allocate: {error}"));
        assert!(atlas.remove(key));
    }

    assert_eq!(atlas.len(), 1, "only the anchor is still cached");
    assert_eq!(atlas.report().textures, 1);
    atlas.flush_uploads(&mut sink).unwrap();
    assert_eq!(
        sink.textures_created(),
        1,
        "a leaking allocator would have needed a new texture long ago"
    );

    atlas.release(anchor);
    assert!(atlas.remove(anchor));
    assert!(atlas.is_empty());
    assert!(atlas.is_fully_reclaimed());
    assert_eq!(atlas.report().textures, 0);
    atlas.flush_uploads(&mut sink).unwrap();
    assert_eq!(sink.live_textures(), 0);
}

#[test]
fn a_refcount_never_underflows_however_often_it_is_released() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(4), || vec![0; 16])
        .unwrap();

    for _ in 0..10 {
        assert!(atlas.release(mono(1)));
    }
    assert_eq!(atlas.refs(mono(1)), Some(0));

    atlas.retain(mono(1));
    assert_eq!(atlas.refs(mono(1)), Some(1));
    atlas.release(mono(1));
    atlas.release(mono(1));
    assert_eq!(atlas.refs(mono(1)), Some(0));
}

#[test]
fn a_saturated_refcount_stays_held_rather_than_wrapping_to_evictable() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(4), || vec![0; 16])
        .unwrap();
    for _ in 0..3 {
        atlas.retain(mono(1));
    }
    assert_eq!(atlas.refs(mono(1)), Some(3));
}

#[test]
fn eviction_frees_exactly_the_least_recently_used_generation() {
    let (mut atlas, _sink) = atlas();

    atlas.begin_frame();
    for handle in 0..3u64 {
        atlas
            .get_or_insert(mono(handle), square(4), || vec![0; 16])
            .unwrap();
    }

    atlas.begin_frame();
    for handle in 3..5u64 {
        atlas
            .get_or_insert(mono(handle), square(4), || vec![0; 16])
            .unwrap();
    }

    // A third frame touching nothing, so the first two generations are both cold.
    atlas.begin_frame();

    let first = atlas.evict_least_recently_used();
    assert_eq!(first.tiles, 3, "exactly the oldest generation");
    assert_eq!(first.texels, 3 * 16);
    assert!((0..3).all(|handle| !atlas.contains(mono(handle))));
    assert!((3..5).all(|handle| atlas.contains(mono(handle))));

    let second = atlas.evict_least_recently_used();
    assert_eq!(second.tiles, 2);
    assert!(atlas.is_empty());

    assert!(atlas.evict_least_recently_used().is_empty());
}

#[test]
fn eviction_spares_what_this_frame_used_and_what_anything_holds() {
    let (mut atlas, _sink) = atlas();

    atlas.begin_frame();
    for handle in 0..3u64 {
        atlas
            .get_or_insert(mono(handle), square(4), || vec![0; 16])
            .unwrap();
    }
    atlas.retain(mono(0));

    atlas.begin_frame();
    assert!(atlas.get(mono(1)).is_some());

    let freed = atlas.evict_all_unused();
    assert_eq!(
        freed.tiles, 1,
        "only the entry that is neither held nor used"
    );
    assert!(atlas.contains(mono(0)));
    assert!(atlas.contains(mono(1)));
    assert!(!atlas.contains(mono(2)));
}

#[test]
fn a_lookup_moves_an_entry_into_the_current_generation() {
    let (mut atlas, _sink) = atlas();
    atlas.begin_frame();
    atlas
        .get_or_insert(mono(1), square(4), || vec![0; 16])
        .unwrap();

    atlas.begin_frame();
    atlas
        .get_or_insert(mono(2), square(4), || vec![0; 16])
        .unwrap();
    assert!(
        atlas.get(mono(1)).is_some(),
        "used again, so no longer cold"
    );

    atlas.begin_frame();
    let freed = atlas.evict_least_recently_used();
    assert_eq!(freed.tiles, 2, "both entries share the second generation");
}

#[test]
fn clearing_destroys_every_texture_and_keeps_the_allocator_consistent() {
    let (mut atlas, mut sink) = atlas();
    for handle in 0..8u64 {
        atlas
            .get_or_insert(mono(handle), square(32), || vec![0; 1024])
            .unwrap();
    }
    atlas.flush_uploads(&mut sink).unwrap();
    assert!(sink.live_textures() > 0);

    atlas.clear();
    atlas.flush_uploads(&mut sink).unwrap();
    assert!(atlas.is_empty());
    assert_eq!(sink.live_textures(), 0);
    assert_eq!(atlas.report(), Default::default());
}

#[test]
fn a_full_pool_reports_out_of_space_so_the_caller_can_evict_and_retry() {
    let limits = AtlasLimits {
        texture_size: 64,
        max_texture_size: 64,
        max_textures_per_pool: 1,
        soft_bytes: None,
    };
    let mut atlas = Atlas::new(limits);
    let _sink = MemorySink::new();

    let mut stored = 0u64;
    let error = loop {
        match atlas.get_or_insert(mono(stored), square(16), || vec![0; 256]) {
            Ok(_) => stored += 1,
            Err(error) => break error,
        }
        assert!(
            stored < 1_000,
            "a 64x64 texture cannot hold this many tiles"
        );
    };
    assert!(matches!(error, AtlasError::OutOfSpace { .. }));

    atlas.begin_frame();
    let freed = atlas.evict_all_unused();
    assert_eq!(freed.tiles as u64, stored);
    assert!(
        atlas
            .get_or_insert(mono(stored), square(16), || vec![0; 256])
            .is_ok(),
        "eviction is what makes the retry work"
    );
}

#[test]
fn the_sink_refuses_a_texture_with_no_extent() {
    let mut sink = MemorySink::new();
    let texture = TextureId::new(TextureKind::Mono, 0);
    assert!(
        sink.create_texture(texture, Size::new(0, 4), TextureFormat::R8Unorm)
            .is_err()
    );
}

proptest! {
    /// Whatever sequence of inserts, retains, releases and removes it is put through, the atlas
    /// never hands out overlapping tiles of one texture, never underflows a count, and returns
    /// every byte of tile space once its entries are gone.
    #[test]
    fn the_invariants_hold_under_any_sequence(
        operations in prop::collection::vec(
            (0u64..16, 0u8..4, 4i32..24),
            0..400,
        )
    ) {
        let mut atlas = Atlas::new(AtlasLimits {
            texture_size: 128,
            max_texture_size: 256,
            max_textures_per_pool: 8,
            soft_bytes: None,
        });
        let sink = MemorySink::new();

        for (handle, operation, side) in operations {
            let key = mono(handle);
            match operation {
                0 => {
                    let bytes = (side * side) as usize;
                    let _ = atlas.get_or_insert(key, square(side), || vec![0; bytes]);
                }
                1 => { atlas.retain(key); }
                2 => { atlas.release(key); }
                _ => { atlas.remove(key); }
            }
            prop_assert!(no_tile_overlaps(&atlas));
        }

        // Releasing everything must make the whole atlas evictable, and eviction must return
        // every last texel of allocated space. A key may have been retained any number of times,
        // so each is released until nothing holds it rather than a fixed number of times.
        for handle in 0..16u64 {
            while atlas.refs(mono(handle)).is_some_and(|held| held > 0) {
                atlas.release(mono(handle));
            }
        }
        atlas.begin_frame();
        atlas.evict_all_unused();
        prop_assert!(atlas.is_empty());
        prop_assert!(atlas.is_fully_reclaimed());
        prop_assert_eq!(sink.live_textures(), 0);
    }
}

/// Whether every pair of live tiles sharing a texture is disjoint.
///
/// This is the property shelf packing exists to provide and the one a mistaken `deallocate` would
/// break: handing a rectangle back out while its previous owner is still drawing from it produces
/// one glyph rendered as another, with no error anywhere.
fn no_tile_overlaps(atlas: &Atlas) -> bool {
    let tiles: Vec<_> = atlas.tiles().collect();
    tiles.iter().enumerate().all(|(index, left)| {
        tiles[index + 1..]
            .iter()
            .all(|right| left.texture != right.texture || !left.bounds.intersects(right.bounds))
    })
}

/// Small textures, so that a handful of tiles is a whole texture's worth of resident bytes.
fn small() -> AtlasLimits {
    AtlasLimits {
        texture_size: 64,
        max_texture_size: 64,
        max_textures_per_pool: 16,
        soft_bytes: None,
    }
}

/// Fills one generation with `count` tiles of `side`, each under its own handle.
fn generation(atlas: &mut Atlas, handles: std::ops::Range<u64>, side: i32) {
    atlas.begin_frame();
    for handle in handles {
        let texels = (side * side) as usize;
        atlas
            .get_or_insert(mono(handle), square(side), || vec![0; texels])
            .unwrap_or_else(|error| panic!("handle {handle} could not allocate: {error}"));
    }
}

#[test]
fn an_atlas_with_no_soft_limit_frees_nothing_of_its_own_accord() {
    let mut atlas = Atlas::new(small());
    let _sink = MemorySink::new();
    generation(&mut atlas, 0..64, 16);
    let resident = atlas.resident_bytes();
    assert!(resident > 0, "the tiles are somewhere");

    // A new frame, so every one of those tiles is cold and nothing holds any of them: they are as
    // evictable as an entry ever gets, and the only thing keeping them is the absent limit.
    atlas.begin_frame();
    let freed = atlas.evict_to_soft_limit();
    assert!(
        freed.is_empty(),
        "an atlas with no stated limit has no criterion for what is too much"
    );
    assert_eq!(atlas.resident_bytes(), resident);
}

#[test]
fn an_atlas_over_its_soft_limit_comes_back_under_it() {
    let one_texture = 64 * 64;
    let mut atlas = Atlas::new(small().with_soft_bytes(2 * one_texture));
    let _sink = MemorySink::new();

    // Four generations, each large enough to fill a texture of its own, so that freeing the cold
    // ones actually gives texture memory back rather than only tile space.
    for round in 0..4u64 {
        generation(&mut atlas, round * 16..round * 16 + 16, 16);
    }
    assert!(
        atlas.resident_bytes() > 2 * one_texture,
        "the frames put it over: a frame is allowed to exceed the limit"
    );

    atlas.begin_frame();
    let freed = atlas.evict_to_soft_limit();
    assert!(!freed.is_empty(), "something had to go");
    assert!(
        atlas.resident_bytes() <= 2 * one_texture,
        "resident bytes are back under the soft limit"
    );
}

#[test]
fn the_soft_limit_never_takes_what_this_frame_drew_or_what_anything_holds() {
    let mut atlas = Atlas::new(small().with_soft_bytes(0));
    let _sink = MemorySink::new();

    generation(&mut atlas, 0..8, 16);
    atlas.retain(mono(0));
    // A second frame that draws one of the old tiles again, so the working set is not empty.
    atlas.begin_frame();
    assert!(atlas.get(mono(1)).is_some());

    let freed = atlas.evict_to_soft_limit();
    assert!(!freed.is_empty());
    assert!(
        atlas.contains(mono(0)),
        "a held tile stays however far over the limit the atlas is"
    );
    assert!(
        atlas.contains(mono(1)),
        "a tile this frame looked up stays: a limit of zero cannot be met and must not be met by \
         evicting what is being drawn"
    );
    assert!(
        atlas.resident_bytes() > 0,
        "the loop stops at the working set rather than spinning"
    );
}

/// The held figure is the tiles' own bytes, and it follows what is held rather than what exists.
///
/// Two tiles of one pool and one held: the count says how many, the byte figure says how much of
/// the budget holding them spends, and the two are not the same question — a thousand held glyphs
/// and one held picture can weigh the same.
#[test]
fn the_report_says_how_many_bytes_the_held_rasters_weigh() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(8), || vec![1; 64])
        .unwrap();
    atlas
        .get_or_insert(mono(2), square(4), || vec![2; 16])
        .unwrap();
    assert_eq!(
        atlas.report().referenced_bytes,
        0,
        "nothing holds either of them yet"
    );

    atlas.retain(mono(1));
    let held = atlas.report();
    assert_eq!(held.referenced_tiles, 1);
    assert_eq!(
        held.referenced_bytes, 64,
        "eight by eight of one byte a texel, and not the four-by-four beside it"
    );

    atlas.retain(mono(2));
    assert_eq!(
        atlas.report().referenced_bytes,
        80,
        "and holding the second adds its own sixteen"
    );

    atlas.release(mono(1));
    assert_eq!(
        atlas.report().referenced_bytes,
        16,
        "releasing one stops it counting, and does not stop the other"
    );
}

/// Lookups are counted, and a lookup that found nothing is not one.
#[test]
fn the_hit_total_counts_lookups_that_found_something() {
    let (mut atlas, _sink) = atlas();
    assert_eq!(atlas.hits(), 0);

    atlas
        .get_or_insert(mono(1), square(8), || vec![1; 64])
        .unwrap();
    let after_insert = atlas.hits();

    assert!(atlas.get(mono(1)).is_some());
    assert!(atlas.get(mono(1)).is_some());
    assert_eq!(
        atlas.hits(),
        after_insert + 2,
        "two lookups of a cached raster are two reads of it"
    );

    let before = atlas.hits();
    assert!(atlas.get(mono(99)).is_none());
    assert_eq!(
        atlas.hits(),
        before,
        "a lookup that found nothing read nothing, so a reader watching this total would be told \
         the atlas was cold — which it was"
    );
}

/// The held totals follow the entries, not the holds taken against them.
///
/// Both halves. A hold on an entry that is then removed — which is what a lost device does to every
/// entry there is — must stop counting, or the figure a budget reads keeps a tile that no longer
/// exists alive forever. And clearing the atlas leaves nothing held, not a total left over from
/// before the device went.
#[test]
fn the_held_totals_do_not_outlive_the_entries_they_count() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(8), || vec![1; 64])
        .unwrap();
    atlas.retain(mono(1));
    assert_eq!(atlas.report().referenced_bytes, 64);

    atlas.remove(mono(1));
    assert_eq!(atlas.report().referenced_tiles, 0);
    assert_eq!(
        atlas.report().referenced_bytes,
        0,
        "an entry that is gone is not being held, whoever still thinks they hold it"
    );

    atlas
        .get_or_insert(mono(2), square(8), || vec![1; 64])
        .unwrap();
    atlas.retain(mono(2));
    atlas.clear();
    assert_eq!(atlas.report(), Default::default());
}

/// Holding one entry twice is one held entry, and one release does not free it.
#[test]
fn a_second_hold_on_one_entry_does_not_count_twice() {
    let (mut atlas, _sink) = atlas();
    atlas
        .get_or_insert(mono(1), square(8), || vec![1; 64])
        .unwrap();
    atlas.retain(mono(1));
    atlas.retain(mono(1));
    assert_eq!(atlas.report().referenced_tiles, 1);
    assert_eq!(atlas.report().referenced_bytes, 64);

    atlas.release(mono(1));
    assert_eq!(
        atlas.report().referenced_bytes,
        64,
        "one of two holds went, so the entry is still held"
    );
    atlas.release(mono(1));
    assert_eq!(atlas.report().referenced_bytes, 0);
}
