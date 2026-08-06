//! The decoded texels the image loader holds.
//!
//! This adapter used to state no level, and its essay said why: the texels arrived from the
//! application already decoded, nothing in the process could produce one again, so every byte was
//! pinned for ever. The loader changed the premise. A picture now arrives *by name* — a path or a
//! bytes URL — and the loader can decode it again, which makes the bytes honestly evictable and
//! the level enforceable.

use zgui_paint::ContentCache;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};
use crate::images::ImageLoader;

/// The pictures this window has decoded, as the budget sees them.
///
/// # What is pinned and what is not
///
/// A source some live element shows is pinned: evicting it would blank a picture that is on the
/// screen until a re-decode lands, which is a flicker no budget is entitled to cause. A source
/// nothing shows — the history a scrolled gallery leaves behind — is the evictable part, and an
/// entry is dropped whole because half a picture is nothing.
///
/// [`forget`](Budgeted::forget) is stronger, as everywhere: it drops the shown ones too, and the
/// loader re-decodes them from their sources on the next settle. The old caveat — that a
/// forgotten window draws blanks until the application re-attaches — is gone; what remains of it
/// is the decode's own latency.
pub struct DecodedImagesBudget<'a> {
    /// Who owns the texels and can produce them again.
    loader: &'a mut ImageLoader,
    /// Where the per-node attachments the eviction has to detach live.
    content: &'a mut ContentCache,
    /// The level sources nothing shows are held under.
    limit: u64,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> DecodedImagesBudget<'a> {
    /// The adapter over one window's decoded images.
    pub(crate) fn new(
        loader: &'a mut ImageLoader,
        content: &'a mut ContentCache,
        limit: u64,
        tracked: &'a mut Tracked,
    ) -> Self {
        Self {
            loader,
            content,
            limit,
            tracked,
        }
    }
}

impl Budgeted for DecodedImagesBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::DecodedImages
    }

    fn limit(&self) -> Option<u64> {
        Some(self.limit)
    }

    fn report(&self) -> CacheReport {
        let held = self.loader.held_bytes();
        let evictable = self.loader.evictable_bytes();
        CacheReport {
            resident: held,
            pinned: held - evictable,
            last_used: self.tracked.last_used(),
            rebuild_cost: rebuild::DECODED,
            speculative: 0,
            unit: CacheUnit::Bytes,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        // A picture is resolved through the content cache on every frame that draws it — there is
        // no replay path that reaches the texels without asking for them — so the lookup total
        // alone is the whole signal.
        self.tracked.note(epoch, self.content.image_hits(), false);
    }

    fn evict(&mut self, units: u64, _epoch: SceneEpoch) -> u64 {
        self.loader.evict(units)
    }

    fn forget(&mut self) {
        self.loader.forget(self.content);
        // Anything an embedder attached around the loader goes too; that half really is
        // unreproducible from here, exactly as the old essay said.
        self.content.forget_images();
    }
}
