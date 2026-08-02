//! The decoded texels attached to replaced nodes.

use zgui_paint::ContentCache;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};

/// The pictures attached to this window's replaced nodes, as the budget sees them.
///
/// # It states no level, and that is the honest answer
///
/// Every byte here is pinned, because nothing in this process can produce one again. The texels
/// arrive from the application already decoded — this framework links no codec, holds no path to
/// the file one came from and has no way to ask for it — so freeing them does not cost a rebuild,
/// it loses the picture until whatever attached it attaches it again.
///
/// A level would therefore be a level that could never be enforced: the first frame over it would
/// report an excess, eviction would free nothing, and the excess would still be there on every
/// frame after. An assertion written against that fails forever, so the number would be raised
/// until it stopped firing — and a number chosen by being raised until it stops firing is not a
/// budget. What bounds this cache is the application, which decides what it attaches.
///
/// It is registered all the same, for the two things registration is for: the report says what the
/// window is spending, and [`forget`](Budgeted::forget) is the one path that does drop these. That
/// is what makes "a window with every cache empty" a state this window can actually be put into,
/// and it is not a memory-pressure step — a caller reaching for it has to expect to attach the
/// pictures again.
///
/// # Nothing in the runtime attaches one yet
///
/// Verified rather than assumed: the only callers of
/// [`ContentCache::set_image`](zgui_paint::ContentCache::set_image) in this workspace are
/// `zgui-paint`'s own tests. An embedder reaches it through the content cache directly, so the
/// cache is real and its bytes are real, and a window driven by this framework's own frame loop
/// reports zero here until one does.
pub struct DecodedImagesBudget<'a> {
    /// Where the texels are.
    content: &'a mut ContentCache,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> DecodedImagesBudget<'a> {
    /// The adapter over one window's attached images.
    pub fn new(content: &'a mut ContentCache, tracked: &'a mut Tracked) -> Self {
        Self { content, tracked }
    }
}

impl Budgeted for DecodedImagesBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::DecodedImages
    }

    /// None, and the reason is on the type.
    fn limit(&self) -> Option<u64> {
        None
    }

    fn report(&self) -> CacheReport {
        let held = self.content.image_bytes();
        CacheReport {
            resident: held,
            // All of it. Nothing here is evictable, ever.
            pinned: held,
            last_used: self.tracked.last_used(),
            rebuild_cost: rebuild::UNREPRODUCIBLE,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        // A picture is resolved through the cache on every frame that draws it — there is no
        // replay path that reaches the texels without asking for them — so the lookup total alone
        // is the whole signal.
        self.tracked.note(epoch, self.content.image_hits(), false);
    }

    /// Nothing, always.
    ///
    /// Not a stub: it is the same answer the report gives, which is that every byte held here is
    /// pinned. A caller that wanted these bytes back wants [`forget`](Budgeted::forget) and has to
    /// accept what that costs.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        0
    }

    fn forget(&mut self) {
        self.content.forget_images();
    }
}
