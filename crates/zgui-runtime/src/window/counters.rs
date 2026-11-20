//! zgui's frame counters, fed from what the compositor and the window's renderer report.
//!
//! The compositor's counters describe its most recent commit and its most recent frame, so they are
//! read right after this window's own commit and its own build. The renderer's named statistics
//! are running totals, so each frame adds what moved since the last one.

use compositor_api::{CommitReport, Compositor, Renderer};
use compositor_types::{FrameCounters, FrameOutcome};
use zgui_paint::SyncReport;
use zgui_profile::{Counter, counter};

/// The renderer's running totals that zgui counts, by the name the renderer reports them under.
const TOTALS: [(&str, Counter); 4] = [
    ("vector_frames_dropped", Counter::VectorFramesDropped),
    ("table_slots_prepared", Counter::SideTableSlotsPrepared),
    ("upload_allocations", Counter::UploadChunksAllocated),
    ("page_writes", Counter::AtlasTextureWrites),
];

/// What one window last read of its renderer's running totals.
#[derive(Debug, Default)]
pub(crate) struct Totals {
    /// The last value of each of [`TOTALS`], in the same order.
    seen: [u64; TOTALS.len()],
}

/// Adds what a commit did to the counters.
pub(crate) fn committed(report: &CommitReport, sync: &SyncReport) {
    counter::add(
        Counter::ChunksIdentical,
        u64::from(report.counters.chunks_identical),
    );
    counter::add(Counter::OrderEntriesDiffed, sync.order_entries as u64);
    counter::add(
        Counter::GlyphsRasterised,
        u64::from(report.counters.glyphs_rasterised),
    );
}

/// Sets the live counters of what `compositor` holds: the tiles in its atlas and the bytes of its
/// chunks.
///
/// The atlas tiles are read from the implementation's `content.live_tiles` statistic, and stay
/// unchanged under a compositor that reports none.
pub(crate) fn held(compositor: &dyn Compositor) {
    counter::set(Counter::PaintChunkBytes, compositor.memory().chunk_bytes);
    compositor.extra_stats(&mut |name, value| {
        if name == "content.live_tiles" {
            counter::set(Counter::AtlasEntriesLive, value);
        }
    });
}

/// Adds what a frame the compositor built did to the counters.
pub(crate) fn built(frame: &FrameCounters) {
    counter::add(
        Counter::PrimitivesEmitted,
        u64::from(frame.items_drawn + frame.items_culled),
    );
    counter::add(Counter::PrimitivesCulled, u64::from(frame.items_culled));
    counter::add(Counter::BoundsTreeInserts, u64::from(frame.order_inserts));
    counter::add(Counter::VelloPasses, u64::from(frame.vector_passes));
    counter::add(Counter::MarksUnionBins, u64::from(frame.mark_bins));
}

impl Totals {
    /// Adds what drawing a frame cost the renderer to the counters.
    pub(crate) fn drawn(&mut self, outcome: &FrameOutcome, renderer: &dyn Renderer) {
        if let Some(stats) = outcome.stats() {
            counter::add(Counter::DrawCalls, u64::from(stats.draw_calls));
            counter::add(Counter::BatchesCulled, u64::from(stats.batches_culled));
            counter::add(Counter::DamagePx, stats.damage_px);
            counter::add(Counter::PresentCopyPx, stats.present_copy_px);
            counter::add(Counter::PresentDamagePx, stats.present_damage_px);
            counter::add(Counter::BytesUploaded, stats.bytes_uploaded);
            counter::add(Counter::ChunkBytesUploaded, stats.chunk_bytes_uploaded);
            counter::add(Counter::GpuTimeNs, stats.gpu_time_ns.unwrap_or(0));
            counter::set(Counter::ChunksResident, u64::from(stats.chunks_resident));
        }
        renderer.extra_stats(&mut |name, value| {
            let Some(index) = TOTALS.iter().position(|(total, _)| *total == name) else {
                return;
            };
            let moved = value.saturating_sub(self.seen[index]);
            self.seen[index] = value;
            counter::add(TOTALS[index].1, moved);
        });
    }
}
