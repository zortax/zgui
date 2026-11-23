//! zgui's frame counters, fed from what the compositor and the window's renderer report.
//!
//! The compositor's counters describe its most recent commit and its most recent frame, so they are
//! read right after this window's own commit and its own build. The renderer's named statistics
//! are running totals, so each frame adds what moved since the last one.

use compositor_api::CommitReport;
use compositor_types::{CompositorEvent, FrameCounters, FrameOutcome};
use zgui_paint::SyncReport;
use zgui_profile::{Counter, counter};

use crate::compositor::Client;

/// What one window last read of its renderer's running totals.
///
/// The totals are the renderer's `vector_frames_dropped`, `table_slots_prepared`,
/// `upload_allocations` and `page_writes` statistics, in that order.
#[derive(Debug, Default)]
pub(crate) struct Totals {
    /// The last value of each total.
    seen: [u64; 4],
}

/// Adds what the sync wrote into a commit to the counters.
pub(crate) fn synced(sync: &SyncReport) {
    counter::add(Counter::OrderEntriesDiffed, sync.order_entries as u64);
}

/// Adds what an applied commit did to the counters.
pub(crate) fn committed(report: &CommitReport) {
    counter::add(
        Counter::ChunksIdentical,
        u64::from(report.counters.chunks_identical),
    );
    counter::add(
        Counter::GlyphsRasterised,
        u64::from(report.counters.glyphs_rasterised),
    );
}

/// Sets the live counters of what the compositor of `client` holds: the tiles in its atlas and
/// the bytes of its chunks.
///
/// The atlas tiles are read from the implementation's `content.live_tiles` statistic, and stay
/// unchanged under a compositor that reports none.
pub(crate) fn held(client: &Client) {
    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    counter::set(Counter::PaintChunkBytes, client.memory().chunk_bytes);
    client.extra_stats(&mut |name, value| {
        if name == "content.live_tiles" {
            counter::set(Counter::AtlasEntriesLive, value);
        }
    });
}

/// Panics when the compositor found a commit's declared damage short of what it changed.
pub(crate) fn check_declared(events: &[CompositorEvent]) {
    for event in events {
        if let CompositorEvent::DamageUndeclared { derived, .. } = event {
            panic!(
                "the frame's paint damage misses pixels its transaction changes: the compositor \
                 derived {derived:?}"
            );
        }
    }
}

/// Adds what a frame the compositor built did to the counters.
pub(crate) fn built(frame: &FrameCounters) {
    counter::add(Counter::PrimitivesEmitted, u64::from(frame.items_drawn));
    counter::add(Counter::PrimitivesCulled, u64::from(frame.items_culled));
    counter::add(Counter::BoundsTreeInserts, u64::from(frame.order_inserts));
    counter::add(Counter::VelloPasses, u64::from(frame.vector_passes));
    counter::add(Counter::MarksUnionBins, u64::from(frame.mark_bins));
}

impl Totals {
    /// Adds what drawing a frame cost the renderer to the counters.
    pub(crate) fn drawn(&mut self, outcome: &FrameOutcome, client: &Client) {
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
        let seen = &mut self.seen;
        client.renderer_stats(&mut |name, value| {
            let mut moved = |index: usize| {
                let moved = value.saturating_sub(seen[index]);
                seen[index] = value;
                moved
            };
            match name {
                "vector_frames_dropped" => counter::add(Counter::VectorFramesDropped, moved(0)),
                "table_slots_prepared" => counter::add(Counter::SideTableSlotsPrepared, moved(1)),
                "upload_allocations" => counter::add(Counter::UploadChunksAllocated, moved(2)),
                "page_writes" => counter::add(Counter::AtlasTextureWrites, moved(3)),
                _ => {}
            }
        });
    }
}
