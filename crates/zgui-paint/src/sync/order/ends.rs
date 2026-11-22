//! A list rebuilt at its ends, as a recycled row changes it.

use zgui_dom::side::BoxKey;

use super::splice::splice;
use super::{Context, Order, Place, push_chunks, push_outline, sorts_by_others};

impl Order {
    /// Rebuilds the list of the box `key`'s node from `children` at its ends, and returns the
    /// entries spliced.
    ///
    /// The list has to keep a run of the children its node lists, and the sync must not have been
    /// given any box of that run to visit. The entries of the run are kept as they are, and only
    /// the children around it are listed again.
    ///
    /// Returns `None` and changes nothing for any other list, and in a sync that visits every
    /// node.
    pub(super) fn rebuild_ends(
        &mut self,
        context: &mut Context<'_>,
        key: BoxKey,
        children: &[BoxKey],
    ) -> Option<usize> {
        if self.scratch.whole || self.volatile.contains(&key) {
            return None;
        }
        let node = self.nodes.get(&key)?;
        if node.spans.len() != node.children.len() + 1 {
            return None;
        }
        let [old_at, new_at, run] = kept_run(&node.children, children)?;
        // A list that keeps every child is the whole rebuild's to compare.
        if run == node.children.len() && run == children.len() {
            return None;
        }
        let kept = &children[new_at..new_at + run];
        let touched = self.scratch.touched.iter().any(|touched| {
            *touched != key
                && self
                    .placed
                    .get(touched)
                    .is_some_and(|place| place.owner == key)
                && kept.contains(touched)
        });
        let store = context.store;
        let (front, back) = (&children[..new_at], &children[new_at + run..]);
        if touched
            || front
                .iter()
                .chain(back)
                .any(|&child| sorts_by_others(store, key, child))
        {
            return None;
        }

        let mut entries = std::mem::take(&mut self.scratch.entries);
        entries.clear();
        let mut spans = std::mem::take(&mut self.scratch.spans);
        spans.clear();
        push_chunks(context.names, store, key, &mut entries);
        for &child in front {
            spans.push(entries.len() as u32);
            self.push_child(context, child, &mut entries);
        }
        let Some(node) = self.nodes.get(&key) else {
            self.scratch.entries = entries;
            self.scratch.spans = spans;
            return None;
        };
        let (from, to) = (node.spans[old_at], node.spans[old_at + run]);
        let at = entries.len() as u32;
        spans.extend(
            node.spans[old_at..old_at + run]
                .iter()
                .map(|span| at + (span - from)),
        );
        entries.extend_from_slice(&node.entries[from as usize..to as usize]);
        for &child in back {
            spans.push(entries.len() as u32);
            self.push_child(context, child, &mut entries);
        }
        spans.push(entries.len() as u32);
        push_outline(context, key, &mut entries);

        // The children that left first, since one of them can come back around the run.
        let Some(node) = self.nodes.get_mut(&key) else {
            self.scratch.entries = entries;
            self.scratch.spans = spans;
            return None;
        };
        let old = std::mem::replace(&mut node.children, children.to_vec());
        for &departed in old[..old_at].iter().chain(&old[old_at + run..]) {
            if self
                .placed
                .get(&departed)
                .is_some_and(|place| place.owner == key)
            {
                self.placed.remove(&departed);
            }
            self.scratch.orphans.push(departed);
        }
        for &child in front.iter().chain(back) {
            let listed = self.needs_node(store, child);
            self.placed.insert(
                child,
                Place {
                    owner: key,
                    node: listed,
                },
            );
            if !listed && self.nodes.contains_key(&child) {
                self.scratch.orphans.push(child);
            }
        }
        let node = self.nodes.get_mut(&key)?;
        let spliced = splice(context.tx, node.id, &node.entries, &entries);
        self.scratch.entries = std::mem::replace(&mut node.entries, entries);
        self.scratch.spans = std::mem::replace(&mut node.spans, spans);
        Some(spliced)
    }
}

/// Returns `[old_at, new_at, length]` of the run of `old` that `new` keeps around its own first or
/// last child, or `None` when it keeps none.
///
/// The run is the one that starts at the first child of `new` or at the first child of `old`,
/// whichever is longer.
fn kept_run(old: &[BoxKey], new: &[BoxKey]) -> Option<[usize; 3]> {
    let length = |old_at: usize, new_at: usize| {
        old[old_at..]
            .iter()
            .zip(&new[new_at..])
            .take_while(|(a, b)| a == b)
            .count()
    };
    let dropped = new
        .first()
        .and_then(|first| old.iter().position(|child| child == first))
        .map(|old_at| [old_at, 0, length(old_at, 0)]);
    let added = old
        .first()
        .and_then(|first| new.iter().position(|child| child == first))
        .map(|new_at| [0, new_at, length(0, new_at)]);
    [dropped, added]
        .into_iter()
        .flatten()
        .filter(|[.., length]| *length > 0)
        .max_by_key(|[.., length]| *length)
}

#[cfg(test)]
mod tests {
    use zgui_dom::side::BoxKey;

    use super::kept_run;

    /// A minted key, for a test that needs a name and not a stored value.
    fn keys(indices: &[u32]) -> Vec<BoxKey> {
        indices
            .iter()
            .map(|index| {
                zgui_arena::Key::new(
                    *index,
                    zgui_arena::Generation::new(1).expect("one is a generation"),
                    zgui_arena::DomainId::FIRST,
                )
            })
            .collect()
    }

    #[test]
    fn a_recycled_list_keeps_the_run_between_its_ends() {
        let old = keys(&[1, 2, 3, 4]);
        assert_eq!(kept_run(&old, &keys(&[2, 3, 4, 5])), Some([1, 0, 3]));
        assert_eq!(kept_run(&old, &keys(&[0, 1, 2, 3])), Some([0, 1, 3]));
        assert_eq!(kept_run(&old, &keys(&[1, 9, 3, 4])), Some([0, 0, 1]));
        assert_eq!(kept_run(&old, &keys(&[7, 8])), None);
    }
}
