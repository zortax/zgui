//! One node of the hierarchy, and how a full one is divided.

use zgui_geom::{Device, DevicePx, Rect};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::MIN_ENTRIES;

/// One node: either a leaf holding entries, or an interior node holding other nodes.
///
/// The link to the node above is stored as well as the links down, because an entry is found by
/// name rather than by searching for its rectangle: knowing which leaf holds it is only half an
/// answer, and the envelopes on the way back to the root are the other half.
#[derive(Debug, Default)]
pub(crate) struct Node {
    /// The rectangle containing everything below this node.
    pub(crate) envelope: Rect<DevicePx, Device>,
    /// The entries, when this is a leaf.
    pub(crate) entries: Vec<(FragKey, Rect<DevicePx, Device>)>,
    /// The child nodes, when it is not.
    pub(crate) children: Option<Vec<usize>>,
    /// The node this one hangs below, which the root has none of.
    pub(crate) parent: Option<usize>,
}

impl Node {
    /// A leaf holding one entry.
    pub(crate) fn leaf(key: FragKey, bounds: Rect<DevicePx, Device>) -> Self {
        Self {
            envelope: bounds,
            entries: vec![(key, bounds)],
            children: None,
            parent: None,
        }
    }

    /// An interior node over the given children.
    pub(crate) fn internal(children: Vec<usize>, envelope: Rect<DevicePx, Device>) -> Self {
        Self {
            envelope,
            entries: Vec::new(),
            children: Some(children),
            parent: None,
        }
    }

    /// A leaf holding the given entries.
    pub(crate) fn leaves(
        entries: Vec<(FragKey, Rect<DevicePx, Device>)>,
        envelope: Rect<DevicePx, Device>,
    ) -> Self {
        Self {
            envelope,
            entries,
            children: None,
            parent: None,
        }
    }

    /// Where in this leaf one entry sits, if it is here at all.
    pub(crate) fn slot_of(&self, key: FragKey) -> Option<usize> {
        self.entries.iter().position(|entry| entry.0 == key)
    }

    /// Whether the node holds nothing at all.
    pub(crate) fn is_empty(&self) -> bool {
        match &self.children {
            None => self.entries.is_empty(),
            Some(children) => children.is_empty(),
        }
    }
}

/// Divides a full node's contents into two groups.
///
/// The rule is the linear one: take the two members furthest apart on whichever axis they are most
/// spread along as the seeds, then give every other member to whichever group its rectangle
/// enlarges less. It is chosen over the quadratic rule deliberately — the quadratic one considers
/// every pair and buys a slightly tighter tree, and with eight entries per node the difference is
/// not measurable while the cost is.
///
/// Neither group is ever left below [`MIN_ENTRIES`]: a split that put one member on one side and
/// the rest on the other would make the tree a list.
pub(crate) fn split<T>(
    mut members: Vec<T>,
    bounds_of: impl Fn(&T) -> Rect<DevicePx, Device>,
) -> (Vec<T>, Vec<T>) {
    let (first, second) = seeds(&members, &bounds_of);
    let moved_member = members.remove(first.max(second));
    let kept_member = members.remove(first.min(second));

    let mut kept_envelope = bounds_of(&kept_member);
    let mut moved_envelope = bounds_of(&moved_member);
    let mut kept = vec![kept_member];
    let mut moved = vec![moved_member];

    let total = members.len() + 2;
    for member in members {
        let bounds = bounds_of(&member);
        let kept_growth = area(kept_envelope.union(bounds)) - area(kept_envelope);
        let moved_growth = area(moved_envelope.union(bounds)) - area(moved_envelope);
        let remaining = total - kept.len() - moved.len();
        let to_kept = if kept.len() + remaining == MIN_ENTRIES {
            true
        } else if moved.len() + remaining == MIN_ENTRIES {
            false
        } else {
            kept_growth <= moved_growth
        };
        if to_kept {
            kept_envelope = kept_envelope.union(bounds);
            kept.push(member);
        } else {
            moved_envelope = moved_envelope.union(bounds);
            moved.push(member);
        }
    }
    (kept, moved)
}

/// The rectangle containing every one of `rects`, or the empty rectangle if there are none.
pub(crate) fn envelope_of(
    rects: impl IntoIterator<Item = Rect<DevicePx, Device>>,
) -> Rect<DevicePx, Device> {
    let mut held: Option<Rect<DevicePx, Device>> = None;
    for rect in rects {
        held = Some(match held {
            Some(union) => union.union(rect),
            None => rect,
        });
    }
    held.unwrap_or(Rect::ZERO)
}

/// A rectangle's area, which is what "grows least" is measured in.
pub(crate) fn area(rect: Rect<DevicePx, Device>) -> f32 {
    (rect.size.width.0.max(0.0)) * (rect.size.height.0.max(0.0))
}

/// The two members that start the groups: the pair furthest apart along one axis.
fn seeds<T>(members: &[T], bounds_of: &impl Fn(&T) -> Rect<DevicePx, Device>) -> (usize, usize) {
    let mut best = (0usize, 1usize);
    let mut best_distance = f32::NEG_INFINITY;
    for (index, member) in members.iter().enumerate() {
        for (other_index, other) in members.iter().enumerate().skip(index + 1) {
            let one = bounds_of(member);
            let two = bounds_of(other);
            let horizontal = (one.origin.x.0 - two.origin.x.0).abs();
            let vertical = (one.origin.y.0 - two.origin.y.0).abs();
            let distance = horizontal + vertical;
            if distance > best_distance {
                best_distance = distance;
                best = (index, other_index);
            }
        }
    }
    best
}
