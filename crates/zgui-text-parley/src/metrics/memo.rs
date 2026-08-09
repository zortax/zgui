//! Face metrics already answered.

use rustc_hash::FxHashMap;
use zgui_geom::CssPx;
use zgui_text::{FaceMetrics, FaceQuery};
use zgui_text_style::{Digest, FontSlant};

/// What one answered metrics query was asked under.
///
/// A hash rather than the query itself, because the query borrows a family list that the answer
/// must outlive, and because a cascade asks the same question for thousands of elements — hashing
/// once is cheaper than comparing family lists thousands of times.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct MemoKey(u64);

impl MemoKey {
    /// The key one query at one size makes.
    pub(crate) fn of(query: &FaceQuery<'_>, size: CssPx, vertical: bool) -> Self {
        let mut digest = Digest::new();
        digest.push(query.family);
        digest.push_f32(query.weight);
        match query.slant {
            FontSlant::Upright => digest.push_tag(0),
            FontSlant::Italic => digest.push_tag(1),
            FontSlant::Oblique(degrees) => {
                digest.push_tag(2);
                digest.push_f32(degrees);
            }
        }
        digest.push_f32(query.width);
        digest.push(query.variations.len());
        for variation in query.variations {
            digest.push(variation.tag);
            digest.push_f32(variation.value);
        }
        digest.push(query.language.map(|language| language.as_str()));
        digest.push_length(size);
        digest.push(vertical);
        Self(digest.finish())
    }
}

/// Metrics answers held across calls.
///
/// Every entry is valid for as long as the set of registered faces is unchanged, which is why
/// registering or dropping a family clears it: the same query would otherwise keep resolving to a
/// face that is no longer the best match, and two elements cascaded either side of the change
/// would disagree about how tall an `ex` is.
#[derive(Debug, Default)]
pub(crate) struct MetricsMemo {
    /// The answers.
    entries: FxHashMap<MemoKey, FaceMetrics>,
    /// The answers that name the face as well as its metrics.
    ///
    /// Held apart from `entries` because the two answer different questions under the same key: one
    /// reports metrics for a query that matched nothing, the other reports that nothing matched.
    resolved: FxHashMap<MemoKey, Option<(zgui_text::FaceId, FaceMetrics)>>,
}

impl MetricsMemo {
    /// The answer held for a key.
    pub(crate) fn get(&self, key: MemoKey) -> Option<FaceMetrics> {
        self.entries.get(&key).copied()
    }

    /// Records an answer.
    pub(crate) fn insert(&mut self, key: MemoKey, metrics: FaceMetrics) {
        self.entries.insert(key, metrics);
    }

    /// The resolved-face answer held for a key.
    pub(crate) fn get_resolved(
        &self,
        key: MemoKey,
    ) -> Option<Option<(zgui_text::FaceId, FaceMetrics)>> {
        self.resolved.get(&key).copied()
    }

    /// Records a resolved-face answer.
    pub(crate) fn insert_resolved(
        &mut self,
        key: MemoKey,
        answer: Option<(zgui_text::FaceId, FaceMetrics)>,
    ) {
        self.resolved.insert(key, answer);
    }

    /// Drops every answer.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.resolved.clear();
    }

    /// How many answers are held.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}
