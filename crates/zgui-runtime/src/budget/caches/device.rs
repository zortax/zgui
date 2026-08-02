//! Everything the renderer holds on the device that no other cache here accounts for.

use zgui_render::Renderer;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The renderer's own device memory, as the budget sees it.
///
/// # Why this is registered when nothing here can free it
///
/// A budget registry that accounts for a megabyte of five hundred is not a budget registry. Every
/// other entry here is a cache — something held because producing it again costs work — and between
/// them they name a small fraction of what a window is actually spending on the device: the pipeline
/// objects, the swapchain, the frame's composition target, the vector rasteriser's scratch and every
/// buffer a frame uploads through are none of them caches, and were therefore none of them counted.
/// The consequence is that the one figure a person asks about, *what is this process holding*, could
/// not be answered from inside the process holding it.
///
/// So this is registered for the report and for nothing else. It states no level, evicts nothing and
/// forgets nothing, because the resources it names are live: the next frame needs every one of them,
/// and a registry entry that freed them would be freeing the thing being drawn into.
///
/// # What it does not claim
///
/// It is the renderer's own accounting, which is what the renderer knows it allocated. It is not the
/// driver's figure for the process, and the gap between the two — allocator padding, the driver's
/// own working set, whatever a compositor holds on this window's behalf — is exactly the quantity a
/// reconciliation against the system's own tool is for. Registering this is what makes that
/// comparison possible at all; it is not what makes it come out even.
pub struct DeviceMemoryBudget<'a> {
    /// What owns the device resources.
    renderer: &'a mut dyn Renderer,
    /// This entry's own history.
    tracked: &'a mut Tracked,
}

impl<'a> DeviceMemoryBudget<'a> {
    /// The adapter over one window's renderer.
    pub fn new(renderer: &'a mut dyn Renderer, tracked: &'a mut Tracked) -> Self {
        Self { renderer, tracked }
    }
}

impl Budgeted for DeviceMemoryBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::DeviceMemory
    }

    /// None, and the reason is on the type.
    fn limit(&self) -> Option<u64> {
        None
    }

    fn report(&self) -> CacheReport {
        // The pooled targets are the one part of this figure another entry already names, and
        // subtracting them is what keeps the registry's total an accounting rather than a sum with
        // one term counted twice.
        let memory = self.renderer.memory();
        let pooled = self.renderer.target_pool().resident;
        let resident = memory.total().saturating_sub(pooled);
        CacheReport {
            resident,
            // All of it. A pipeline, a swapchain and the target a frame is composed into are live
            // resources of the window rather than remembered results of past work.
            pinned: resident,
            last_used: self.tracked.last_used(),
            rebuild_cost: rebuild::UNREPRODUCIBLE,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        self.tracked.note(epoch, 1, false);
    }

    /// Nothing, always. See the note on the type.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        0
    }

    /// Nothing, always, for the same reason.
    fn forget(&mut self) {}
}
