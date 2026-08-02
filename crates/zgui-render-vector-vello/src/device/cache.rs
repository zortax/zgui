//! The path renderer's compiled-pipeline blob, kept between runs.
//!
//! It is a **first-launch** saving rather than a per-launch one. Building the renderer costs a few
//! milliseconds once a driver has a shader cache of its own, and a great deal more the very first
//! time a machine ever runs this program — which is precisely why a benchmark that means to measure
//! this has to clear the driver's own cache first, or it measures nothing and passes.

use std::path::PathBuf;

use zgui_render_wgpu::Gpu;

/// The environment variable naming where the blob is kept.
pub const DIRECTORY_VARIABLE: &str = "ZGUI_CACHE_DIR";

/// A compiled-pipeline cache, or nothing where the device offers none.
#[derive(Debug, Default)]
pub struct PipelineCache {
    /// The cache itself.
    cache: Option<wgpu::PipelineCache>,
    /// Where its contents are kept.
    path: Option<PathBuf>,
}

impl PipelineCache {
    /// The handle the renderer is built with.
    pub fn handle(&self) -> Option<&wgpu::PipelineCache> {
        self.cache.as_ref()
    }

    /// Writes the blob back, if there is one.
    ///
    /// Failure costs the next launch some compilation and nothing else, so it is logged rather than
    /// reported: refusing to start over an unwritable cache directory would be absurd.
    pub fn store(&self) {
        let (Some(cache), Some(path)) = (&self.cache, &self.path) else {
            return;
        };
        let Some(data) = cache.get_data() else {
            return;
        };
        if let Some(directory) = path.parent()
            && let Err(error) = std::fs::create_dir_all(directory)
        {
            tracing::debug!(?error, "the pipeline cache directory could not be created");
            return;
        }
        if let Err(error) = std::fs::write(path, data) {
            tracing::debug!(?error, "the pipeline cache could not be written");
        }
    }
}

/// Loads the blob for `gpu`'s adapter, or nothing if there is none to load.
///
/// It is keyed by the adapter's own identity, because a blob written by one driver is not merely
/// useless to another — it is data that driver would have to refuse.
pub fn load(gpu: &Gpu) -> PipelineCache {
    if !gpu
        .device()
        .features()
        .contains(wgpu::Features::PIPELINE_CACHE)
    {
        return PipelineCache::default();
    }
    let Some(path) = path_for(gpu) else {
        return PipelineCache::default();
    };
    let data = std::fs::read(&path).ok();
    let cache = create(gpu, data.as_deref());
    PipelineCache {
        cache: Some(cache),
        path: Some(path),
    }
}

/// Creates the cache object itself.
///
/// The one place a raw blob is handed to a driver. It is safe as written: the bytes are either
/// absent or ones this program wrote for an adapter with this exact identity, the graphics library
/// validates the blob's own header before the driver sees it, and `fallback` is set — so a blob the
/// driver rejects costs a recompilation rather than a failure.
#[expect(
    unsafe_code,
    reason = "creating a pipeline cache from a stored blob has no safe spelling"
)]
fn create(gpu: &Gpu, data: Option<&[u8]>) -> wgpu::PipelineCache {
    // SAFETY: see this function's own documentation — the blob is this program's own, written for
    // an adapter with the same identity, header-validated before the driver reads it, and marked
    // `fallback` so a rejection is a recompilation.
    unsafe {
        gpu.device()
            .create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: Some("zgui.vector.pipeline_cache"),
                data,
                fallback: true,
            })
    }
}

/// Where `gpu`'s blob lives, or `None` when the adapter has no stable identity to key it by.
fn path_for(gpu: &Gpu) -> Option<PathBuf> {
    let key = wgpu::util::pipeline_cache_key(&gpu.adapter().get_info())?;
    Some(directory()?.join(format!("vector-{key}")))
}

/// The directory blobs are kept in.
fn directory() -> Option<PathBuf> {
    if let Ok(named) = std::env::var(DIRECTORY_VARIABLE) {
        return Some(PathBuf::from(named));
    }
    let home = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok()?;
    Some(home.join("zgui").join("pipelines"))
}
