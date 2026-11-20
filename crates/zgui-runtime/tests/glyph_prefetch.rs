//! The first frame's glyphs, rasterised on the layout pool before the emit walk.
//!
//! A cold window with a pool rasterises the glyphs its first frame needs on the workers, and the
//! walk takes the answers where it asks for them. The walk still allocates each tile where it asks,
//! so the frame must draw the same sprites from the same tiles as a window with no pool. A sprite
//! carries its tile, so equal sprites are equal tile ids and equal places in the atlas.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use zgui_platform_headless::Harness;
use zgui_profile::counter;
use zgui_runtime::App;
use zgui_scene::MonoSprite;
use zgui_text::{GlyphImage, GlyphKey, GlyphOutline, GlyphRaster, OutlineKey};
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .row { display: block }";

/// Sizes with an advance off the pixel grid, so one letter lands at more than one phase.
const SIZES: [u32; 6] = [9, 11, 13, 15, 17, 19];

/// The mono rasteriser, counting the keys asked for one at a time.
///
/// The walk asks one at a time and the prefetch asks in batches, so this count is the keys the
/// walk found no prefetched answer for.
struct Counted {
    inner: zgui_testkit_scene::MonoRaster,
    single: Arc<AtomicU64>,
}

impl GlyphRaster for Counted {
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage> {
        self.single.fetch_add(1, Ordering::Relaxed);
        self.inner.raster(key)
    }

    fn raster_many(&self, keys: &[GlyphKey]) -> Vec<Option<GlyphImage>> {
        keys.iter().map(|key| self.inner.raster(key)).collect()
    }

    fn outline(&self, key: &OutlineKey) -> Option<GlyphOutline> {
        self.inner.outline(key)
    }
}

/// What one first frame drew and what it cost.
struct FirstFrame {
    sprites: Vec<MonoSprite>,
    rasterised: u64,
    prefetched: u64,
    /// Keys the rasteriser got one at a time.
    single: u64,
}

/// Opens a window over `rows` rows of text with a layout pool `threads` wide, and reads its first
/// frame.
fn first_frame(threads: usize, rows: usize) -> FirstFrame {
    let before = counter::snapshot();
    let single = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&single);
    let handler = App::new()
        .with_title("prefetch")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_layout_threads(threads)
        .with_renderer(Box::new(support::capture))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(move || {
            Arc::new(Counted {
                inner: zgui_testkit_scene::MonoRaster::new(),
                single: Arc::clone(&counted),
            })
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut list = zgui_elements::column();
            for row in 0..rows {
                let size = SIZES[row % SIZES.len()];
                list = list.child(
                    zgui_elements::column()
                        .class("row")
                        .style_text(Some(format!("font-size: {size}px")))
                        .child(format!("{row} the quick brown fox jumps over the lazy dog")),
                );
            }
            Box::new(list.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(64);
    let moved = before.delta(&counter::snapshot());
    FirstFrame {
        sprites: harness.app().windows()[0]
            .scene()
            .primitives
            .mono_sprites
            .clone(),
        rasterised: moved.glyphs_rasterised,
        prefetched: moved.glyphs_prefetched,
        single: single.load(Ordering::Relaxed),
    }
}

/// Whether the environment fixes the pool width, which makes the two windows the same.
fn pool_width_is_fixed() -> bool {
    std::env::var_os("ZGUI_LAYOUT_THREADS").is_some()
}

#[test]
fn a_prefetched_first_frame_draws_the_serial_frame() {
    if pool_width_is_fixed() || !zgui_profile::COUNTERS_ENABLED {
        eprintln!("skipped: the environment sets the pool width, or counters are off");
        return;
    }
    let _turn = counter::exclusive();
    let serial = first_frame(1, 40);
    let pooled = first_frame(4, 40);

    assert_eq!(serial.prefetched, 0, "a window with no pool prefetched");
    assert!(
        pooled.prefetched >= 128,
        "the pooled window did not prefetch, so this compares the serial path with itself: {}",
        pooled.prefetched
    );
    assert_eq!(
        pooled.single, 0,
        "the walk found no prefetched answer for {} of {} keys",
        pooled.single, pooled.rasterised
    );
    assert_eq!(serial.single, serial.rasterised);
    assert_eq!(
        pooled.rasterised, serial.rasterised,
        "the walk asks for each key once whichever path answers it"
    );
    assert!(!serial.sprites.is_empty(), "the first frame drew no glyph");
    assert_eq!(
        pooled.sprites, serial.sprites,
        "the prefetched frame drew different sprites or read different tiles"
    );
}

#[test]
fn a_small_first_frame_stays_serial() {
    if pool_width_is_fixed() || !zgui_profile::COUNTERS_ENABLED {
        eprintln!("skipped: the environment sets the pool width, or counters are off");
        return;
    }
    let _turn = counter::exclusive();
    let small = first_frame(4, 1);

    assert!(small.rasterised > 0, "the first frame rasterised nothing");
    assert_eq!(
        small.prefetched, 0,
        "{} misses went to the pool, which costs more than it saves",
        small.rasterised
    );
}
