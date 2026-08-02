//! The metrics seam, answered from a memo in front of the collection.

use zgui_geom::CssPx;
use zgui_text::{FaceMetrics, FaceQuery, FontMetricsSource};
use zgui_text_style::GenericFamily;

use crate::metrics::memo::MemoKey;
use crate::metrics::read::face_metrics;
use crate::system::FontSystem;

/// The default size a proportional family starts from.
pub const BASE_SIZE: CssPx = CssPx(16.0);

/// The default size a monospace family starts from.
///
/// Smaller than [`BASE_SIZE`] because a monospace face at the same nominal size reads larger, and
/// every environment configures the two separately for that reason.
pub const MONOSPACE_BASE_SIZE: CssPx = CssPx(13.0);

impl FontMetricsSource for FontSystem {
    fn face_metrics(&self, query: &FaceQuery<'_>, size: CssPx, vertical: bool) -> FaceMetrics {
        let key = MemoKey::of(query, size, vertical);
        if let Some(answer) = self
            .memo
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
        {
            return answer;
        }
        let metrics = self.read_metrics(query, size, vertical);
        self.memo
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, metrics);
        metrics
    }

    fn base_size(&self, generic: GenericFamily) -> CssPx {
        match generic {
            GenericFamily::Monospace => MONOSPACE_BASE_SIZE,
            _ => BASE_SIZE,
        }
    }
}

impl FontSystem {
    /// How many distinct metrics queries have been answered and remembered.
    ///
    /// Read beside [`lock_acquisitions`](FontSystem::lock_acquisitions): the collection is reached
    /// once per distinct query, so the two numbers stay close to each other however many times the
    /// cascade asks.
    pub fn metrics_memo_len(&self) -> usize {
        self.memo
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    /// Drops every remembered metrics answer.
    ///
    /// Every answer is valid only for as long as the set of registered faces is unchanged, so this
    /// is what a family arriving or leaving costs.
    pub fn forget_metrics(&self) {
        self.memo
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// Resolves the face and reads its metrics, taking the collection's lock exactly once.
    ///
    /// A query that matches no face at all reports the default metrics — every optional field
    /// absent and a zero ascent — rather than failing. A document styled with a family nothing
    /// provides still has to cascade, and each font-relative unit then resolves against its
    /// documented fallback fraction.
    fn read_metrics(&self, query: &FaceQuery<'_>, size: CssPx, vertical: bool) -> FaceMetrics {
        let families = crate::font::resolve::families(query);
        let attributes = crate::font::resolve::attributes(query);
        let variations = query.variations.to_vec();
        self.locked(|shared| {
            let mut answer = None;
            let crate::system::shared::Shared {
                collection,
                sources,
                ..
            } = shared;
            let mut lookup = collection.query(sources);
            lookup.set_families(families.iter().map(|entry| match entry {
                crate::font::resolve::QueryEntry::Named(name) => {
                    fontique::QueryFamily::Named(name.as_str())
                }
                crate::font::resolve::QueryEntry::Generic(generic) => {
                    fontique::QueryFamily::Generic(*generic)
                }
            }));
            lookup.set_attributes(attributes);
            lookup.matches_with(|font| {
                answer = face_metrics(font.blob.data(), font.index, size, &variations, vertical);
                fontique::QueryStatus::Stop
            });
            answer.unwrap_or_default()
        })
    }
}
