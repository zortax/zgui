//! The driver's compiled-pipeline cache, kept between runs.
//!
//! Compiling the pipeline set costs a fraction of a second on a machine that has never run this
//! program, and almost nothing afterwards, because the driver keeps a cache of its own. So this is
//! a **first-launch** saving and a saving on drivers that keep no cache — which is exactly why a
//! benchmark that means to measure it has to clear the driver's cache first, or it measures
//! nothing and passes.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Whether a pipeline was built with the cache since it was last written.
    unsaved: Cell<bool>,
    /// The length of the blob last loaded or written.
    ///
    /// The driver's data grows only when it compiles a pipeline, so data of this length holds
    /// nothing new.
    written: Cell<usize>,
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
        let written = data.as_ref().map_or(0, Vec::len);
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
            unsaved: Cell::new(false),
            written: Cell::new(written),
        }
    }

    /// The handle a pipeline is built with.
    ///
    /// Only a pipeline build asks for it, so a call marks the cache as holding unsaved data.
    pub fn handle(&self) -> Option<&wgpu::PipelineCache> {
        if self.cache.is_some() {
            self.unsaved.set(true);
        }
        self.cache.as_ref()
    }

    /// Whether a pipeline was built with the cache since it was last written.
    #[cfg(test)]
    pub(crate) fn unsaved(&self) -> bool {
        self.unsaved.get()
    }

    /// Whether the cache has a device cache and a path to keep it at.
    #[cfg(test)]
    pub(crate) fn persists(&self) -> bool {
        self.cache.is_some() && self.path.is_some()
    }

    /// Writes the cache back when a pipeline was built since the last write and the driver's data
    /// changed length.
    ///
    /// A pipeline that a frame builds on demand, such as one for a group target format, is in the
    /// next launch's cache only through this write. Without it, that launch compiles the pipeline
    /// again in its first frame whenever the driver's own cache misses.
    ///
    /// The blob goes to a temporary file first and then replaces the old one, so a launch that
    /// reads it meanwhile sees a whole blob. Failure is logged and otherwise ignored: a cache that
    /// could not be written costs the next launch some compilation.
    pub fn store(&self) {
        let (Some(cache), Some(path)) = (&self.cache, &self.path) else {
            return;
        };
        if !self.unsaved.replace(false) {
            return;
        }
        let Some(data) = cache.get_data() else {
            return;
        };
        if data.len() == self.written.get() {
            return;
        }
        if let Some(directory) = path.parent()
            && let Err(error) = std::fs::create_dir_all(directory)
        {
            tracing::debug!(?error, "the pipeline cache directory could not be created");
            return;
        }
        // Unique to this write, because renderers in several threads and processes share the path.
        static WRITES: AtomicU64 = AtomicU64::new(0);
        let write = WRITES.fetch_add(1, Ordering::Relaxed);
        let staged = path.with_extension(format!("{}.{write}.tmp", std::process::id()));
        let length = data.len();
        match std::fs::write(&staged, data).and_then(|()| std::fs::rename(&staged, path)) {
            Ok(()) => self.written.set(length),
            Err(error) => {
                let _ = std::fs::remove_file(&staged);
                tracing::debug!(?error, "the pipeline cache could not be written");
            }
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
