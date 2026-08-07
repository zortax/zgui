//! The rasteriser contract.

use zgui_scene::ScenePassPlan;

use crate::memory::MemoryReport;
use crate::vector::error::VectorError;
use crate::vector::frame::VectorFrame;
use crate::vector::plan::VectorPlan;

/// The implementation that executes general vector passes.
///
/// Small solid paths may bypass this entirely by becoming atlas masks in the paint stage. This
/// names the backend only for paths that survive as vector items in the display list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorBackend {
    /// The compute path rasteriser from Vello.
    Vello,
    /// zgui's portable coverage rasteriser.
    Coverage,
    /// A renderer-specific or test implementation.
    Other,
}

/// Whether a renderer has a general vector rasteriser and whether its lazy cost has been paid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VectorStatus {
    /// Which backend is configured, when one is configured.
    pub backend: Option<VectorBackend>,
    /// Whether the backend has been constructed rather than merely configured for lazy creation.
    pub initialized: bool,
}

/// Rasterises vector items into scratch coverage for a compositing draw to read.
///
/// An implementation produces **straight** — that is, un-premultiplied — colour, and the compositing
/// draw premultiplies. Content outside a requested region is left untouched.
///
/// # What is and is not decided here
///
/// Which items survive, where one pass ends and the next begins, what each pass clips through and
/// whether it may be composited one item at a time are all decided in the display list, before any
/// of this runs. An implementation decides only its own resources.
///
/// In particular an implementation **must not cull against damage**: the plan it is handed has
/// already been culled, and a second cull would mean two owners of one decision — with the pass
/// count becoming a number only a real device could produce, rather than something a test can assert
/// about a scene.
///
/// # Residual clips are part of the contract
///
/// One composite applies one clip, so an item whose chain runs deeper than its pass's has the extra
/// links applied *inside* the scratch. Every implementation must honour that; there is no fallback
/// to specify, because anything an implementation could not express was already turned into a pass
/// boundary before the plan was made.
///
/// # The frame methods, and why
///
/// The frame calls every one of them, and a trait declaring fewer than its only caller uses is a
/// seam with one implementation wearing a trait's clothes.
///
/// * [`plan`](VectorRaster::plan) resources the work.
/// * [`clear_targets`](VectorRaster::clear_targets) is mandatory rather than an optimisation: an
///   implementation whose rasterisation can fail while reporting success would otherwise leave a
///   reused scratch holding the *previous* frame's content, which composites as wrong pixels rather
///   than missing ones and has nothing to notice it by.
/// * [`prepare`](VectorRaster::prepare) does the work, before the frame's own recording begins,
///   because an implementation may submit work of its own.
/// * [`memory`](VectorRaster::memory) is what a budget is written against.
///
/// [`backend`](VectorRaster::backend) is diagnostic metadata rather than frame work: the inspector
/// reads it without causing a rasterisation or initializing a lazy rasteriser.
pub trait VectorRaster: 'static {
    /// Which general vector backend this is.
    ///
    /// Defaulted for test and application-specific rasterisers; built-in implementations identify
    /// themselves so diagnostics never need to downcast them.
    fn backend(&self) -> VectorBackend {
        VectorBackend::Other
    }

    /// Turns the display list's plan into whatever this implementation needs to execute it.
    ///
    /// Returns an empty plan when nothing survived, and the caller is expected to do nothing at all
    /// in that case: an empty pass over a full-size surface is not free.
    ///
    /// **The result is index-aligned with what it was given**: one [`VectorPass`](crate::VectorPass) per
    /// [`PlannedPass`](zgui_scene::PlannedPass), in the same order, or an empty plan. The display
    /// list names each composite by the *plan's* index, so an implementation that dropped a pass it
    /// did not like would not draw one composite fewer — it would draw every later composite from
    /// the wrong pass. [`VectorPlan::resourcing`] starts a plan that keeps the alignment.
    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan;

    /// Clears every scratch the plan will write, before anything reads it.
    fn clear_targets(&mut self, plan: &VectorPlan);

    /// Rasterises every pass of the frame.
    ///
    /// Called before the frame's own command recording begins.
    ///
    /// # Errors
    ///
    /// [`VectorError`] when the work could not be completed. A caller that continues anyway draws a
    /// frame whose vector content is missing, which is why an implementation must report a capacity
    /// overflow rather than swallowing it.
    fn prepare(&mut self, frame: &mut VectorFrame<'_>) -> Result<(), VectorError>;

    /// What this implementation is currently holding.
    fn memory(&self) -> MemoryReport;

    /// Releases reproducible scratch after a wall-clock idle grace period.
    ///
    /// Implementations keep their initialized pipelines and fixed state resident; this only sheds
    /// allocations a later vector frame can recreate.
    fn release_idle_resources(&mut self) -> u64 {
        0
    }
}
