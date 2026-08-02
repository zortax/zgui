//! A property group's identity, in a form that cannot be handed out twice.

use core::fmt;
use core::hash::{Hash, Hasher};

use servo_arc::Arc as ServoArc;
use style::properties::style_structs;

use crate::computed::style::ComputedStyle;

/// The identity of one property group's allocation, holding a reference to it.
///
/// [`StructPtr`](crate::StructPtr) answers the same question as a bare number, and a bare number is
/// only an identity for as long as the allocation behind it is alive. A style whose last reference
/// is dropped frees its groups, the allocator hands the same addresses to the next style built, and
/// anything that stored the old number now answers the new group with the old group's result —
/// every property wrong, no error anywhere.
///
/// This is the same identity with that failure removed. Holding one keeps the group alive, so the
/// address it compares by cannot be reissued while any handle to it exists. The cost is that a
/// table keyed on these pins one property group per live key, so a long-lived table needs a sweep
/// or a clear; the alternative is a table that is silently wrong.
///
/// ```
/// use zgui_css::{PinnedGroup, StyleDraft};
///
/// let style = StyleDraft::initial().build();
/// let one = PinnedGroup::inherited_text(&style);
/// let same = PinnedGroup::inherited_text(&style);
/// assert_eq!(one, same, "one group has one identity");
///
/// let other = PinnedGroup::inherited_text(&StyleDraft::initial().build());
/// assert_ne!(one.addr(), 0, "a live group is never at the null address");
/// let _ = other;
/// ```
pub struct PinnedGroup<T> {
    /// The group itself, held so that its address stays this handle's alone.
    group: ServoArc<T>,
}

impl<T> PinnedGroup<T> {
    /// The address the handle compares by.
    ///
    /// Useful for logging and for interoperating with a key that is already a number; it is *not*
    /// a substitute for the handle, because a number kept after the handle is dropped is exactly
    /// the reuse this type exists to prevent.
    pub fn addr(&self) -> usize {
        self.group.heap_ptr() as usize
    }
}

impl PinnedGroup<style_structs::InheritedText> {
    /// The identity of a style's inherited-text group — colour, spacing, alignment, indent,
    /// wrapping and white-space handling.
    pub fn inherited_text(style: &ComputedStyle) -> Self {
        Self {
            group: style.clone_inherited_text(),
        }
    }
}

impl<T> Clone for PinnedGroup<T> {
    fn clone(&self) -> Self {
        Self {
            group: ServoArc::clone(&self.group),
        }
    }
}

impl<T> PartialEq for PinnedGroup<T> {
    fn eq(&self, other: &Self) -> bool {
        self.addr() == other.addr()
    }
}

impl<T> Eq for PinnedGroup<T> {}

impl<T> Hash for PinnedGroup<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.addr().hash(state);
    }
}

impl<T> PartialOrd for PinnedGroup<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for PinnedGroup<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.addr().cmp(&other.addr())
    }
}

impl<T> fmt::Debug for PinnedGroup<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PinnedGroup")
            .field("addr", &self.addr())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::PinnedGroup;
    use crate::computed::draft::StyleDraft;

    /// The property the type exists for: an address held here cannot come back as another group's.
    ///
    /// The loop is what forces the reuse. Each style it builds is a temporary, so it is freed
    /// before the next is allocated, and an allocator that hands the same block back is the common
    /// case rather than the exotic one. Keeping every handle is what stops that from happening.
    #[test]
    fn a_held_identity_is_never_reissued_to_another_group() {
        let mut held = Vec::new();
        for _ in 0..256 {
            held.push(PinnedGroup::inherited_text(&StyleDraft::initial().build()));
        }

        let mut addresses: Vec<usize> = held.iter().map(PinnedGroup::addr).collect();
        addresses.sort_unstable();
        let before = addresses.len();
        addresses.dedup();
        assert_eq!(
            addresses.len(),
            before,
            "two live groups reported the same address, so the handle is not pinning anything",
        );
    }

    /// The control for the test above: dropping the handles is what lets the address come back, so
    /// the reuse this type prevents is real and not hypothetical on this allocator.
    #[test]
    fn dropping_the_handles_does_let_an_address_come_back() {
        let mut seen = std::collections::HashSet::new();
        let mut repeats = 0;
        for _ in 0..256 {
            let address = PinnedGroup::inherited_text(&StyleDraft::initial().build()).addr();
            if !seen.insert(address) {
                repeats += 1;
            }
        }
        assert!(
            repeats > 0,
            "no address was ever reused, so this allocator cannot demonstrate the hazard",
        );
    }
}
