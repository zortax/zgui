//! The collection, the faces it has issued handles for, and the one lock over both.

use fontique::{Collection, CollectionOptions, SourceCache};
use rustc_hash::FxHashMap;

use crate::font::color::ColorSupport;
use crate::font::registry::FaceTable;
use crate::system::options::FontSystemOptions;

/// Everything that needs exclusive access, behind one lock.
///
/// The collection's own API takes `&mut self` throughout — even for a pure lookup like
/// `family_by_name` — while the cascade asks for face metrics from several threads at once behind
/// a shared reference. One lock and a memo in front of it is what reconciles those, and the memo
/// is the half that matters: it is what keeps the lock off the path of almost every call.
pub(crate) struct Shared {
    /// The families and faces.
    pub(crate) collection: Collection,
    /// The file cache the collection loads face bytes through.
    pub(crate) sources: SourceCache,
    /// The handles this system has issued.
    pub(crate) faces: FaceTable,
    /// Which colour mechanisms each face carries, keyed by file identity and face index.
    colors: FxHashMap<(u64, u32), ColorSupport>,
}

impl Shared {
    /// A collection set up as `options` asks.
    ///
    /// The collection is created *shared* so that a clone handed to the shaper sees faces this
    /// system registers afterwards. Without that, `@font-face` would install a face the metrics
    /// source could resolve and the shaper could not, which is a difference no test of either half
    /// alone can see.
    pub(crate) fn new(options: FontSystemOptions) -> Self {
        Self {
            collection: Collection::new(CollectionOptions {
                shared: true,
                system_fonts: options.enumeration.reads_the_system(),
            }),
            sources: SourceCache::default(),
            faces: FaceTable::default(),
            colors: FxHashMap::default(),
        }
    }

    /// The colour mechanisms one face carries, read once per file and remembered.
    pub(crate) fn color_support(&mut self, key: (u64, u32), data: &[u8]) -> ColorSupport {
        if let Some(support) = self.colors.get(&key) {
            return *support;
        }
        let support = ColorSupport::probe(data, key.1);
        self.colors.insert(key, support);
        support
    }
}
