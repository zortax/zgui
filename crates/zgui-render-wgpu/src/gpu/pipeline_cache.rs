//! The driver's compiled-pipeline cache, kept between runs.
//!
//! Compiling the pipeline set costs a fraction of a second on a machine that has never run this
//! program, and almost nothing afterwards, because the driver keeps a cache of its own. So this is
//! a **first-launch** saving and a saving on drivers that keep no cache — which is exactly why a
//! benchmark that means to measure it has to clear the driver's cache first, or it measures
//! nothing and passes.

use std::path::PathBuf;

use crate::gpu::device::Gpu;

/// The environment variable naming where the cache blob is kept.
pub const DIRECTORY_VARIABLE: &str = "ZGUI_CACHE_DIR";

/// A compiled-pipeline cache, or nothing where the device offers none.
#[derive(Debug, Default)]
pub struct PipelineCache {
    /// The cache itself, when the device supports one.
    cache: Option<wgpu::PipelineCache>,
    /// Where its contents are kept.
    path: Option<PathBuf>,
}

impl PipelineCache {
    /// Loads the cache for `gpu`'s adapter, or nothing if there is none to load.
    ///
    /// The blob is keyed by the adapter's identity, because a blob written by one driver is not
    /// merely useless to another — it is data that driver will refuse or misread.
    pub fn load(gpu: &Gpu) -> Self {
        if !gpu
            .device()
            .features()
            .contains(wgpu::Features::PIPELINE_CACHE)
        {
            return Self::default();
        }
        let Some(path) = path_for(gpu) else {
            return Self::default();
        };
        let data = std::fs::read(&path).ok();
        // SAFETY: the data is either absent or the bytes this program previously wrote for an
        // adapter with this exact identity, and wgpu validates the blob's own header before the
        // driver sees it. `fallback` is set, so a blob the driver rejects costs a recompilation
        // rather than a failure.
        let cache = unsafe {
            gpu.device()
                .create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("zgui.pipeline_cache"),
                    data: data.as_deref(),
                    fallback: true,
                })
        };
        Self {
            cache: Some(cache),
            path: Some(path),
        }
    }

    /// The handle a pipeline is built with.
    pub fn handle(&self) -> Option<&wgpu::PipelineCache> {
        self.cache.as_ref()
    }

    /// Writes the cache back, if there is one.
    ///
    /// Failure is logged and otherwise ignored: a cache that could not be written costs the next
    /// launch some compilation, and refusing to start over it would be absurd.
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

/// Where `gpu`'s cache blob lives, or `None` when the adapter has no stable identity to key it by.
fn path_for(gpu: &Gpu) -> Option<PathBuf> {
    let key = wgpu::util::pipeline_cache_key(&gpu.adapter().get_info())?;
    Some(directory()?.join(key))
}

/// The directory cache blobs are kept in.
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
