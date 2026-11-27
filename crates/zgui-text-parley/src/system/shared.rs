//! The collection, the faces it has issued handles for, and the one lock over both.

use fontique::{Collection, CollectionOptions, SourceCache};

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
            // Shared, so every context cloned from this system — the frame's shaper and each of its
            // forks — resolves one font file to one blob. An unshared cache gives each clone its
            // own copy of the data, and everything keyed by blob identity downstream — the face table
            // above all — then treats the same face as a different font per shaper.
            sources: SourceCache::new_shared(),
            faces: FaceTable::default(),
        }
    }
}
