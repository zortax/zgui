//! Generation-counted eviction.

use crate::atlas::Atlas;
use crate::key::AtlasKey;

/// What one eviction freed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Eviction {
    /// The generation that was freed, or `None` when there was nothing to free.
    pub generation: Option<u64>,
    /// How many entries went.
    pub tiles: usize,
    /// How many texels of tile space came back.
    pub texels: u64,
}

impl Eviction {
    /// Whether nothing was freed.
    pub const fn is_empty(&self) -> bool {
        self.tiles == 0
    }
}

impl Atlas {
    /// Frees exactly the coldest generation of evictable entries.
    ///
    /// An entry is evictable when nothing holds it — see [`Atlas::retain`] — and when this frame
    /// has not looked it up. Of those, the ones sharing the oldest generation go, and no others:
    /// eviction is a step down one generation rather than a sweep to a watermark, so a caller that
    /// needs more room calls it again and can see what each step bought.
    ///
    /// The frame's own working set is safe from it by construction, because looking an entry up is
    /// what marks it.
    pub fn evict_least_recently_used(&mut self) -> Eviction {
        self.evict_least_recently_used_into(&mut Vec::new())
    }

    /// The detailed form of [`Atlas::evict_least_recently_used`], appending the keys removed.
    pub fn evict_least_recently_used_into(&mut self, removed: &mut Vec<AtlasKey>) -> Eviction {
        let Some(generation) = self.evictable().map(|(_, age, _)| age).min() else {
            return Eviction::default();
        };
        let doomed: Vec<(AtlasKey, u64)> = self
            .evictable()
            .filter(|(_, age, _)| *age == generation)
            .map(|(key, _, texels)| (key, texels))
            .collect();

        let mut eviction = Eviction {
            generation: Some(generation),
            tiles: 0,
            texels: 0,
        };
        for (key, texels) in doomed {
            if self.remove(key) {
                removed.push(key);
                eviction.tiles += 1;
                eviction.texels += texels;
            }
        }
        eviction
    }

    /// Frees every evictable entry, generation by generation.
    ///
    /// Returns the total, and stops at the frame's working set and at anything held, exactly as one
    /// step of [`Atlas::evict_least_recently_used`] does.
    pub fn evict_all_unused(&mut self) -> Eviction {
        let mut total = Eviction::default();
        loop {
            let step = self.evict_least_recently_used();
            if step.is_empty() {
                return total;
            }
            total.generation = step.generation;
            total.tiles += step.tiles;
            total.texels += step.texels;
        }
    }

    /// Frees cold generations until [`Atlas::resident_bytes`] is back under the soft limit.
    ///
    /// Nothing at all happens when [`AtlasLimits::soft_bytes`](crate::AtlasLimits::soft_bytes) is
    /// unset: an atlas with no stated limit has no criterion for what is too much, and freeing a
    /// tile for no reason costs a re-rasterisation for no reason.
    ///
    /// Generation by generation from the coldest, and it stops the moment a step frees nothing —
    /// which is the state where everything left is either held or in this frame's working set. A
    /// frame whose own working set is larger than the limit therefore stays over it rather than
    /// evicting what it is about to draw.
    ///
    /// Resident bytes fall only when a whole texture empties, so the loop is over textures rather
    /// than tiles and a single step may free a great many tiles and no bytes at all.
    pub fn evict_to_soft_limit(&mut self) -> Eviction {
        self.evict_to_soft_limit_into(&mut Vec::new())
    }

    /// The detailed form of [`Atlas::evict_to_soft_limit`], appending the keys removed.
    pub fn evict_to_soft_limit_into(&mut self, removed: &mut Vec<AtlasKey>) -> Eviction {
        let Some(soft) = self.limits.soft_bytes else {
            return Eviction::default();
        };
        let mut total = Eviction::default();
        while self.resident_bytes() > soft {
            let step = self.evict_least_recently_used_into(removed);
            if step.is_empty() {
                break;
            }
            total.generation = step.generation;
            total.tiles += step.tiles;
            total.texels += step.texels;
        }
        total
    }

    /// Every evictable entry, as its key, its generation and the texels it occupies.
    fn evictable(&self) -> impl Iterator<Item = (AtlasKey, u64, u64)> + '_ {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(slot, held)| held.as_ref().map(|entry| (slot, entry)))
            .filter(|(slot, entry)| entry.is_unreferenced() && !self.used.contains(*slot))
            .map(|(_, entry)| {
                let texels = entry.size.width.max(0) as u64 * entry.size.height.max(0) as u64;
                (entry.key, entry.generation, texels)
            })
    }
}
