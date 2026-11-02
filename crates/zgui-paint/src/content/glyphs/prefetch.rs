//! Glyphs rasterised on the layout workers before the emit walk asks for them.
//!
//! A cold window rasterises every glyph it shows in its first frame, one at a time, on the frame
//! thread. Hinting is most of that cost, and each glyph is independent of the others. The prefetch
//! reads the keys the walk will ask for off the fragment tree, rasterises them on the pool, and
//! keeps the answers in the glyph cache. It runs only while the cache is empty, so every predicted
//! key is a miss.
//!
//! The walk is not changed. It asks for each key where it did before, and takes the prefetched
//! answer in place of a call to the rasteriser. The atlas insert stays at that point in the walk,
//! so every tile is allocated in the same order and at the same place as without a prefetch. A
//! prediction that is wrong costs only the work: the walk rasterises a key it does not find, and an
//! answer it never asks for is dropped at the end of the frame.

use rustc_hash::FxHashSet;
use zgui_layout::FragmentKind;
use zgui_layout::tree::parallel::LayoutPool;
use zgui_layout::tree::store::LayoutStore;
use zgui_profile::{Counter, counter};
use zgui_text::{
    GlyphKey, GlyphRaster, PenPosition, RasterPath, RasterStyle, RunSurface, ShapedGlyphs,
};

/// The fewest predicted misses that go to the pool.
///
/// One glyph costs about 15 µs to hint and rasterise. Below this count the serial walk finishes
/// before the workers wake and each prepares its own hinting state.
pub(crate) const MIN_MISSES: usize = 128;

/// The most glyphs the prediction reads.
///
/// Reading one glyph costs about 80 ns, so this caps the prediction near 1.3 ms. A long document
/// has few distinct keys past its first lines, and the walk rasterises those few itself.
const MAX_VISITED: usize = 16_384;

/// How many batches each worker gets.
///
/// More than one, so a worker that gets the expensive glyphs does not hold up the others. Few, so
/// each batch prepares the hinting state of few faces and sizes.
const BATCHES_PER_WORKER: usize = 2;

/// What the walk will draw the glyphs with.
#[derive(Clone, Copy, Debug)]
pub struct GlyphPrediction {
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
    /// Whether the device can draw per-channel coverage and the surface is opaque.
    ///
    /// The walk also requires each run to sit on the pixel grid. The prediction assumes it does.
    pub subpixel: bool,
}

/// The keys the walk is expected to rasterise, in no particular order.
///
/// The lines of the fragment tree in store order, each placed where its fragment is, at the style
/// an untransformed run of one colour gets, until [`MAX_VISITED`] glyphs are read. The caller
/// calls it only while the glyph cache is empty, so each distinct key is a miss.
pub(crate) fn predict(
    store: &LayoutStore,
    shaped: &dyn ShapedGlyphs,
    prediction: GlyphPrediction,
) -> Vec<GlyphKey> {
    let surface = RunSurface {
        scale: prediction.scale,
        ..RunSurface::PLAIN
    };
    let mut seen: FxHashSet<GlyphKey> = FxHashSet::default();
    let mut misses = Vec::new();
    let mut visited = 0;
    store.each_fragment(|fragment| {
        if visited >= MAX_VISITED {
            return;
        }
        let (FragmentKind::Line { paragraph, line }
        | FragmentKind::TextRun {
            paragraph,
            run: line,
        }) = fragment.kind
        else {
            return;
        };
        let Some(key) = store.paragraph_key(paragraph) else {
            return;
        };
        // The origin the walk places the line from: the fragment's own border box.
        let origin = fragment.border_box.origin.x.0;
        shaped.visit_line(key, line, &mut |run| {
            if run.raster_path(surface) == RasterPath::Vector {
                return;
            }
            let style = run.raster_style(prediction.subpixel);
            visited += run.glyphs.len();
            for glyph in run.glyphs {
                let position = PenPosition::of(origin + glyph.x);
                let key = run.key_at(glyph.glyph, position.offset(), style);
                if seen.insert(key) {
                    misses.push(key);
                }
            }
        });
    });
    misses
}

/// Rasterises `keys` on the pool, and returns each key with its answer.
///
/// The keys are sorted by face and size first, so each batch prepares the hinting state of few
/// faces. The answers do not depend on the order or on the batch: equal keys give identical bytes.
pub(crate) fn rasterise(
    mut keys: Vec<GlyphKey>,
    raster: &dyn GlyphRaster,
    pool: &LayoutPool,
) -> Vec<(GlyphKey, Option<zgui_text::GlyphImage>)> {
    keys.sort_unstable_by_key(|key| {
        (
            key.face,
            key.instance,
            key.size_bits,
            key.synthetic_bold_bits,
            key.synthetic_slant_bits,
            style_rank(key.style),
            key.offset,
            key.glyph,
        )
    });
    let batches = (pool.width() * BATCHES_PER_WORKER).max(1);
    let per_batch = keys.len().div_ceil(batches);
    let mut chunks: Vec<Vec<GlyphKey>> = Vec::with_capacity(batches);
    let mut rest = keys.as_slice();
    while !rest.is_empty() {
        let (head, tail) = rest.split_at(per_batch.min(rest.len()));
        chunks.push(head.to_vec());
        rest = tail;
    }
    let answered = pool.map(chunks, |chunk| {
        let images = raster.raster_many(&chunk);
        chunk.into_iter().zip(images).collect::<Vec<_>>()
    });
    counter::add(Counter::GlyphsPrefetched, keys.len() as u64);
    answered.into_iter().flatten().collect()
}

/// A sort rank for a raster style, which has no order of its own.
fn style_rank(style: RasterStyle) -> u8 {
    match style {
        RasterStyle::Grayscale => 0,
        RasterStyle::Subpixel => 1,
        RasterStyle::Color => 2,
    }
}
