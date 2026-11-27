//! The font system: one collection, one lock, and the memo that keeps the lock cold.

pub mod generics;
pub mod options;
pub(crate) mod shared;

pub use crate::system::options::{Enumeration, FontSystemOptions};

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock};

use crate::metrics::memo::MetricsMemo;
use crate::system::shared::Shared;

/// The faces a document draws with, and the metrics its cascade resolves against.
///
/// One of these is built per application and shared by every consumer: the cascade reads face
/// metrics through it on several threads at once, the shaper draws faces from the same collection,
/// and `@font-face` registers into it.
///
/// # What the lock is for, and why almost nothing takes it
///
/// A font collection needs exclusive access even to answer a question, while the cascade asks it
/// questions from every worker thread through a shared reference. So the collection sits behind a
/// lock and every answer that can be remembered is: repeated face-metrics queries are served from
/// a memo keyed on the query and the size, and [`lock_acquisitions`](FontSystem::lock_acquisitions)
/// is what makes the claim checkable rather than aspirational.
///
/// ```
/// use zgui_geom::CssPx;
/// use zgui_text::{FaceQuery, FontMetricsSource};
/// use zgui_text_parley::{FontSystem, FontSystemOptions};
/// use zgui_text_style::TextStyle;
///
/// let fonts = FontSystem::new(FontSystemOptions::registered_only());
/// let style = TextStyle::initial();
/// let query = FaceQuery::of(&style);
///
/// let first = fonts.face_metrics(&query, CssPx(16.0), false);
/// let taken = fonts.lock_acquisitions();
/// // The second ask is the memo's, not the collection's.
/// assert_eq!(fonts.face_metrics(&query, CssPx(16.0), false), first);
/// assert_eq!(fonts.lock_acquisitions(), taken);
/// ```
pub struct FontSystem {
    /// The collection and the face table.
    shared: Mutex<Shared>,
    /// Face metrics already answered.
    pub(crate) memo: RwLock<MetricsMemo>,
    /// How many times the collection's lock has been taken.
    locks: AtomicU64,
    /// How the system was built.
    options: FontSystemOptions,
}

impl FontSystem {
    /// A font system set up as `options` asks.
    pub fn new(options: FontSystemOptions) -> Self {
        Self {
            shared: Mutex::new(Shared::new(options)),
            memo: RwLock::new(MetricsMemo::default()),
            locks: AtomicU64::new(0),
            options,
        }
    }

    /// How the system was built.
    pub fn options(&self) -> FontSystemOptions {
        self.options
    }

    /// How many times the collection's lock has been taken since the system was built.
    ///
    /// Every operation that must reach the collection itself takes it exactly once; everything
    /// answered from a memo takes it not at all. A number that tracks the call count rather than
    /// the number of distinct queries means a memo has stopped working.
    pub fn lock_acquisitions(&self) -> u64 {
        self.locks.load(Ordering::Relaxed)
    }

    /// Runs `with` against the collection, counting the acquisition.
    ///
    /// A poisoned lock is recovered from rather than propagated: a panic inside one metrics query
    /// leaves the collection readable, and turning every later query into a panic would take an
    /// application down for a font it could simply have failed to find.
    pub(crate) fn locked<T>(&self, with: impl FnOnce(&mut Shared) -> T) -> T {
        self.locks.fetch_add(1, Ordering::Relaxed);
        let mut guard: MutexGuard<'_, Shared> = match self.shared.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        with(&mut guard)
    }

    /// A font context sharing this system's collection.
    ///
    /// The collection is shared rather than copied, so a face registered through this system
    /// afterwards is visible to everything built from this context. The file cache is *not*
    /// shared, because it is a scratch space rather than state: two of them hold the same bytes
    /// twice at worst, while sharing one would put the shaper behind the metrics lock.
    pub fn font_context(&self) -> parley::FontContext {
        self.locked(|shared| parley::FontContext {
            collection: shared.collection.clone(),
            source_cache: shared.sources.clone(),
        })
    }
}

impl core::fmt::Debug for FontSystem {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("FontSystem")
            .field("options", &self.options)
            .field("lock_acquisitions", &self.lock_acquisitions())
            .finish()
    }
}
