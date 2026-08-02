//! Lowering once per distinct style rather than once per element.
//!
//! # Why there are two lookups and not one
//!
//! The fast one is by identity: two elements that cascaded to the same result share the very same
//! allocations, so equal group addresses are proof of equal values and the lookup is a handful of
//! integer tests. A thousand identically styled buttons take that path and lower once.
//!
//! It is not the only one, and the reason is structural rather than theoretical. The cascade runs
//! across several worker threads and each worker builds its own sharing cache, so *n* workers can
//! produce *n* distinct allocations for one logical style — and a cache keyed only on identity
//! would lower once per worker. So an identity miss lowers, hashes the *lowering*, and on a content
//! hit throws its own work away and aliases the new identity onto the entry that already exists.
//! The hash is paid only on the path that had already paid for a lowering.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_css::ComputedStyle;
use zgui_profile::{Counter, counter};
use zgui_scene::content::ContentHash;

use crate::lower::key::LoweringKey;
use crate::lower::{PaintStyle, lower};

/// A reference to one lowered style held by a [`PaintStyleCache`].
///
/// It is an index rather than a pointer so that it is `Copy` and comparable, which is what lets it
/// be recorded beside a fragment's cached paint operations: comparing this frame's reference with
/// the one a fragment was last painted with is how the emitter decides whether the fragment can be
/// replayed instead of encoded again.
///
/// # Why it carries which cache it is an index into
///
/// An index alone is an identity only for as long as the thing it indexes keeps its contents. This
/// cache is emptied and refilled whenever the device scale moves, because every length in a
/// lowering is in device pixels — and the refilling is driven by the *damage*, so which styles are
/// lowered and in what order is a property of what the frame happened to paint. Index three is
/// therefore one style before a scale change and, quite legitimately, a different one after it.
///
/// A bare index recorded beside a fragment's operations survives that. The record is compared
/// against the reference the fragment lowers to now, the two numbers are equal, and the fragment is
/// replayed — re-emitting the range it was encoded into, which draws the colours of a style it no
/// longer has. Nothing downstream can see it: the display list says what it says, the damage was
/// computed correctly, and the pixels are simply the ones from before the window changed monitors.
///
/// The generation is what makes the comparison mean what it reads as. A reference from a cache that
/// has since been emptied compares equal to nothing, so the fragment is encoded again, and
/// [`PaintStyleCache::get`] answers `None` for it rather than answering with whatever now occupies
/// that index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaintStyleRef {
    /// Which lifetime of the cache the index is an index into.
    generation: u32,
    /// The lowering's position in that lifetime.
    index: u32,
}

impl PaintStyleRef {
    /// The `index`th lowering of the `generation`th filling of a cache.
    ///
    /// Public for the tests that build a record by hand; a caller with a cache gets one from
    /// [`PaintStyleCache::lower`] instead.
    pub const fn new(generation: u32, index: u32) -> Self {
        Self { generation, index }
    }

    /// The lowering's position in the cache that issued it.
    pub const fn index(self) -> u32 {
        self.index
    }
}

/// The lowered styles a document has needed, and the two ways of finding one again.
#[derive(Debug)]
pub struct PaintStyleCache {
    /// The lowerings themselves, addressed by [`PaintStyleRef`].
    lowered: Vec<PaintStyle>,
    /// The identity lookup: the fast path.
    by_identity: FxHashMap<LoweringKey, PaintStyleRef>,
    /// The content lookup: the fallback that makes the bound per document rather than per worker.
    ///
    /// A hash narrows the search and equality settles it, because a hash collision that aliased two
    /// different styles would paint one element with another's colours and report nothing.
    by_content: FxHashMap<u64, SmallVec<[PaintStyleRef; 1]>>,
    /// The device scale every lowering here was performed at.
    scale: f32,
    /// How many times the cache has been emptied, which is what a reference carries so that one
    /// issued before an emptying cannot be mistaken for one issued after it.
    generation: u32,
}

impl Default for PaintStyleCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintStyleCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self {
            lowered: Vec::new(),
            by_identity: FxHashMap::default(),
            by_content: FxHashMap::default(),
            scale: 1.0,
            generation: 0,
        }
    }

    /// Which lifetime of this cache references are being issued in.
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// How many distinct lowerings are held.
    pub fn len(&self) -> usize {
        self.lowered.len()
    }

    /// Whether nothing has been lowered yet.
    pub fn is_empty(&self) -> bool {
        self.lowered.is_empty()
    }

    /// The lowering `reference` names, or `None` for one this cache no longer issues.
    ///
    /// A reference from a previous lifetime answers `None` rather than answering with whatever has
    /// since taken that index: the index is still in range, and that is exactly the failure — an
    /// element painted with another element's colours, with nothing anywhere reporting it.
    pub fn get(&self, reference: PaintStyleRef) -> Option<&PaintStyle> {
        if reference.generation != self.generation {
            return None;
        }
        self.lowered.get(reference.index as usize)
    }

    /// Discards everything.
    ///
    /// Every length in a lowering is in device pixels, so a change of device scale makes every
    /// entry wrong. It is a whole-cache event because it is a whole-document event.
    pub fn clear(&mut self) {
        self.lowered.clear();
        self.by_identity.clear();
        self.by_content.clear();
        // Every reference handed out before now indexes a list that is about to stop existing, and
        // the indices are handed out again from zero. This is what stops one of them comparing equal
        // to a reference issued afterwards — see [`PaintStyleRef`].
        self.generation = self.generation.wrapping_add(1);
    }

    /// The lowering of `style` at `scale`, performing it only if it has not been performed before.
    pub fn lower(&mut self, style: &ComputedStyle, scale: f32) -> PaintStyleRef {
        if scale != self.scale {
            self.clear();
            self.scale = scale;
        }
        let key = LoweringKey::of(style);
        if let Some(reference) = self.by_identity.get(&key) {
            counter::bump(Counter::StylesLoweredFromCache);
            return *reference;
        }
        let lowered = lower(style, scale);
        let hash = hash_of(&lowered, scale);
        let existing = self.by_content.get(&hash).and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|reference| self.lowered[reference.index() as usize] == lowered)
        });
        if let Some(reference) = existing {
            counter::bump(Counter::StylesLoweredFromCache);
            self.by_identity.insert(key, reference);
            return reference;
        }
        counter::bump(Counter::StylesLowered);
        let reference = PaintStyleRef::new(self.generation, self.lowered.len() as u32);
        self.by_content.entry(hash).or_default().push(reference);
        self.lowered.push(lowered);
        self.by_identity.insert(key, reference);
        reference
    }
}

/// A hash of everything two lowerings would be compared on.
///
/// It hashes bit patterns rather than values, which is what makes it exact and stable: the question
/// is "are these the same description", not "are these numerically close".
fn hash_of(style: &PaintStyle, scale: f32) -> u64 {
    let mut hash = ContentHash::new()
        .u32(u32::from(style.visible))
        .u32(u32::from(style.transform_forces_group))
        .u32(style.clip_path as u32)
        .f32(scale);
    hash = color(hash, style.color);
    hash = color(hash, style.background.color);
    for layer in &style.background.layers {
        hash = hash
            .u32(u32::from(layer.repeating))
            .u32(layer.interpolation.space as u32)
            .u32(layer.interpolation.hue as u32)
            .u64(layer.stops.len() as u64);
        for stop in &layer.stops {
            hash = color(hash, stop.color).u32(u32::from(stop.position.is_some()));
        }
    }
    for value in style.border.colors {
        hash = color(hash, value);
    }
    hash = hash
        .u32(style.border.style as u32)
        .u32(u32::from(style.border.invisible));
    for shadow in style.shadows.iter().chain(style.text_shadows.iter()) {
        hash = color(hash, shadow.color)
            .f32s(&[
                shadow.offset_x,
                shadow.offset_y,
                shadow.deviation,
                shadow.spread,
            ])
            .u32(u32::from(shadow.inset));
    }
    if let Some(outline) = &style.outline {
        hash = color(hash, outline.color)
            .f32s(&[outline.width, outline.offset])
            .u32(outline.style as u32);
    }
    hash = hash
        .f32(style.group.opacity)
        .u32(u32::from(style.group.isolated))
        .u64(style.group.blend.mix as u64)
        .u64(style.group.blend.compose as u64);
    for filter in style
        .group
        .filters
        .iter()
        .chain(style.group.backdrop.iter())
    {
        let (left, top, right, bottom) = filter.kernel_support();
        hash = hash.f32s(&[left, top, right, bottom]);
    }
    style.decoration.fold_into(hash).finish()
}

/// Folds a colour's space and channels into a hash.
fn color(hash: ContentHash, color: zgui_color::Color) -> ContentHash {
    hash.u32(color.space() as u32)
        .f32s(&color.components())
        .f32(color.alpha())
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::PaintStyleCache;
    use crate::lower::key::LoweringKey;

    #[test]
    fn one_style_is_lowered_once_however_often_it_is_asked_for() {
        let mut cache = PaintStyleCache::new();
        let style = StyleDraft::initial().build();
        let first = cache.lower(&style, 1.0);
        for _ in 0..100 {
            assert_eq!(cache.lower(&style, 1.0), first);
        }
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn two_separate_allocations_of_one_style_share_a_lowering() {
        // This is the multi-worker case: two cascade results equal in value and different in
        // identity. Without the content fallback the cache would hold two entries for one style.
        let mut cache = PaintStyleCache::new();
        let one = StyleDraft::initial().build();
        let other = StyleDraft::initial().build();
        assert_ne!(
            LoweringKey::of(&one),
            LoweringKey::of(&other),
            "the fixture has to produce two allocations, or the fallback is not exercised"
        );
        assert_eq!(cache.lower(&one, 1.0), cache.lower(&other, 1.0));
        assert_eq!(cache.len(), 1);
    }

    /// The failure the generation exists for, written as the two frames that produce it.
    ///
    /// A reference is recorded beside a fragment's operations on one frame and compared against the
    /// one the fragment lowers to on the next. Between the two the window moved to a monitor at
    /// another scale, so the cache was emptied and filled again — and it fills in the order the
    /// *damage* reaches styles, which is not the order it filled in before. A bare index would
    /// compare equal here and the fragment would be replayed: the same operations, drawing the
    /// colours of a style that is no longer its own, with correct damage and nothing to report.
    #[test]
    fn a_reference_does_not_survive_the_cache_being_emptied_under_it() {
        let mut cache = PaintStyleCache::new();
        let one = StyleDraft::initial().build();
        let recorded = cache.lower(&one, 1.0);
        assert_eq!(recorded.index(), 0);

        // The window moves to another monitor. Everything is lowered again, and the first style
        // this frame reaches takes the index the recorded one had.
        let again = cache.lower(&one, 1.2);
        assert_eq!(again.index(), 0, "the fixture has to reuse the index");
        assert_ne!(
            recorded, again,
            "a reference from the emptied cache compared equal to one issued after it, so the \
             fragment holding it would be replayed with the wrong style's colours"
        );
        assert!(
            cache.get(recorded).is_none(),
            "an emptied cache answered a stale reference with whatever now occupies that index"
        );
        assert!(cache.get(again).is_some());
    }

    #[test]
    fn a_change_of_scale_discards_every_lowering() {
        let mut cache = PaintStyleCache::new();
        let style = StyleDraft::initial().build();
        cache.lower(&style, 1.0);
        assert_eq!(cache.len(), 1);
        cache.lower(&style, 2.0);
        assert_eq!(
            cache.len(),
            1,
            "the entry at the old scale went rather than accumulating"
        );
    }
}
