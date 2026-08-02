//! Lowering once per distinct style rather than once per element.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_css::{ComputedStyle, StructPtr};
use zgui_profile::{Counter, counter};

use crate::lower::set::{TextStyleSet, style_set};

/// Which property groups a lowering depends on.
///
/// Only three of the twenty groups say anything about text, so two elements that differ in their
/// background, their borders and their margins still share a key — which is what makes the hit rate
/// on this cache a property of how many *distinct text styles* a document has rather than of how
/// many elements it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextStyleKey {
    /// The font group.
    pub font: StructPtr,
    /// The inherited-text group.
    pub inherited_text: StructPtr,
    /// The inherited-box group, which carries the writing direction.
    pub inherited_box: StructPtr,
}

impl TextStyleKey {
    /// The key of one style.
    pub fn of(style: &ComputedStyle) -> Self {
        Self {
            font: StructPtr::font(style),
            inherited_text: StructPtr::inherited_text(style),
            inherited_box: StructPtr::inherited_box(style),
        }
    }
}

/// Lowered styles, held against the identity of the groups they came from.
///
/// A thousand buttons with one style lower once. The cache is *keyed on identity*, so it answers
/// only when the groups are literally the same allocations — which is the common case, because the
/// cascade shares a group between every element that resolved to it.
///
/// It is not a complete answer on its own, and the reason is worth knowing before relying on the
/// hit rate: styles are cascaded on several threads, and two threads can build separate allocations
/// holding identical values. A miss therefore means "not seen on this pointer", never "different
/// style", and the cost of a miss is one lowering.
///
/// # Why an entry holds its style
///
/// A key made of addresses is only an identity for as long as those addresses cannot be handed out
/// twice. A style whose last reference is dropped frees its property groups, the allocator hands
/// the same addresses to the next style built, and a cache holding only the numbers would answer
/// the new style with the old one's lowering — every property wrong, no error anywhere. So an entry
/// keeps a reference to the style it was made from, which pins the groups its key names for exactly
/// as long as the key is live. [`clear`](TextStyleCache::clear) is what releases them.
///
/// ```
/// use zgui_css::StyleDraft;
/// use zgui_text_style::TextStyleCache;
///
/// let mut cache = TextStyleCache::default();
/// let style = StyleDraft::initial().build();
///
/// let first = cache.get(&style);
/// let again = cache.get(&style);
/// assert!(std::sync::Arc::ptr_eq(&first, &again), "the same style lowers once");
/// assert_eq!(cache.hits(), 1);
/// assert_eq!(cache.lowerings(), 1);
/// ```
#[derive(Debug, Default)]
pub struct TextStyleCache {
    /// The lowerings held.
    entries: FxHashMap<TextStyleKey, Entry>,
    /// How many calls were answered from `entries`.
    hits: u64,
    /// How many calls performed a lowering.
    lowerings: u64,
}

/// One held lowering, beside the style whose addresses its key is made of.
#[derive(Debug)]
struct Entry {
    /// Kept, never read: holding it is what stops the groups the key names from being freed and
    /// their addresses reissued to a different style.
    _style: ComputedStyle,
    /// The lowering itself.
    lowered: Arc<TextStyleSet>,
}

impl TextStyleCache {
    /// The lowering of one style, performing it if this is the first time these groups are seen.
    ///
    /// A call that performs a lowering moves [`Counter::StylesLowered`] and one answered from the
    /// cache moves [`Counter::StylesLoweredFromCache`], so the hit rate over a frame is readable
    /// from the process-wide counters without a handle on the cache that produced it.
    pub fn get(&mut self, style: &ComputedStyle) -> Arc<TextStyleSet> {
        let key = TextStyleKey::of(style);
        if let Some(held) = self.entries.get(&key) {
            self.hits += 1;
            counter::bump(Counter::StylesLoweredFromCache);
            return Arc::clone(&held.lowered);
        }
        self.lowerings += 1;
        counter::bump(Counter::StylesLowered);
        let lowered = Arc::new(style_set(style));
        self.entries.insert(
            key,
            Entry {
                _style: style.clone(),
                lowered: Arc::clone(&lowered),
            },
        );
        lowered
    }

    /// How many calls were answered without lowering anything.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// How many lowerings were performed.
    pub fn lowerings(&self) -> u64 {
        self.lowerings
    }

    /// How many distinct lowerings are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops every held lowering, and with it every style being kept alive to pin a key.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
