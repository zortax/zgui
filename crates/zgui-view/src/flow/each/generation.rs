//! The scopes a keyed list's items hang off, and how they are retired.

use zgui_reactive::Owner;

/// One generation of item scopes.
struct Generation {
    /// The scope the items of this generation are children of, until it is dropped.
    owner: Option<Owner>,
    /// How many items have ever been created under it.
    created: u32,
    /// How many of those are still live.
    live: u32,
}

/// The generations a keyed list keeps.
///
/// Every item of a list gets its own scope, so removing one item frees exactly that item's
/// signals, memos and cleanups. Those per-item scopes hang off a generation scope, and the reason
/// generations exist at all is a property of the reactive engine that a long-lived list runs into
/// and nothing else does:
///
/// **A scope keeps a weak reference to every child it has ever had, and dropping a child does not
/// remove it.** So a list that has created ten thousand rows over its life has ten thousand
/// entries under its scope, of which ten are live, and disposing of that scope walks all ten
/// thousand. The cost is in items-ever-created, not in items-live.
///
/// The fix is to retire a generation and start a new one, and the shape of it is not the obvious
/// shape:
///
/// * **Retirement never re-parents.** Existing items stay exactly where they are; only items
///   created afterwards land in the new generation. Re-parenting is not expressible — the engine's
///   scope type has no re-parent and no child removal — and the drop it would need is precisely
///   the operation that would dispose of every live row.
/// * **A new generation's parent is the list's own scope, never the generation it replaces.**
///   Chaining each generation under its predecessor would mean that dropping the predecessor, which
///   happens when its last item dies, disposes of every live row in every generation after it.
/// * **Dead-reference counting is the list's own bookkeeping.** The engine exposes neither a
///   scope's children nor their number, so "created minus live" is the only measurable proxy — and
///   it is exact for the quantity that matters.
///
/// What remains is bounded: dead references are at most `2 × live + generations`, and generations
/// accumulate at roughly one per `live` items ever created.
pub(super) struct Generations {
    /// The list's own scope, which every generation is a child of.
    root: Owner,
    /// The generations, oldest first, with the current one last.
    generations: Vec<Generation>,
}

impl Generations {
    /// Starts with one generation under `root`.
    pub(super) fn new(root: Owner) -> Self {
        let first = Generation {
            owner: Some(root.child()),
            created: 0,
            live: 0,
        };
        Self {
            root,
            generations: vec![first],
        }
    }

    /// How many generations are being kept.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.generations.len()
    }

    /// Which generation new items are being created in.
    pub(super) fn current(&self) -> usize {
        self.generations.len() - 1
    }

    /// A scope for a new item, and the generation it belongs to.
    pub(super) fn scope_for_new_item(&mut self) -> (usize, Owner) {
        let at = self.current();
        let generation = &mut self.generations[at];
        generation.created += 1;
        generation.live += 1;
        let owner = generation
            .owner
            .as_ref()
            .expect("the current generation is never dropped")
            .child();
        (at, owner)
    }

    /// Records that an item of `generation` has gone away.
    ///
    /// A retired generation whose last item goes has its scope dropped, at which point the engine
    /// finds only dead references under it and cleans nothing.
    pub(super) fn item_dropped(&mut self, generation: usize) {
        let is_current = generation == self.current();
        let Some(entry) = self.generations.get_mut(generation) else {
            return;
        };
        entry.live = entry.live.saturating_sub(1);
        if entry.live == 0 && !is_current {
            entry.owner = None;
        }
    }

    /// Starts a new generation when the current one has accumulated more dead references than the
    /// whole list has live items.
    ///
    /// Called once per reconciliation, after the removals and the insertions.
    pub(super) fn retire_if_needed(&mut self, live_items: u32) {
        let at = self.current();
        let generation = &self.generations[at];
        if generation.created.saturating_sub(generation.live) > live_items {
            self.generations.push(Generation {
                owner: Some(self.root.child()),
                created: 0,
                live: 0,
            });
        }
    }

    /// Disposes of every generation, which disposes of every item still under one.
    pub(super) fn dispose(&mut self) {
        for generation in self.generations.drain(..) {
            if let Some(owner) = generation.owner {
                owner.cleanup();
            }
        }
        self.generations.push(Generation {
            owner: Some(self.root.child()),
            created: 0,
            live: 0,
        });
    }

    /// How many generations still hold a live scope.
    #[cfg(test)]
    pub(super) fn live_generations(&self) -> usize {
        self.generations
            .iter()
            .filter(|generation| generation.owner.is_some())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use zgui_reactive::{Owner, install};

    use super::Generations;

    #[test]
    fn a_new_generation_is_started_once_the_dead_outnumber_the_live() {
        install().ok();
        let root = Owner::new();
        let mut generations = Generations::new(root);

        // Ten items created, nine gone: nine dead against one live.
        for _ in 0..10 {
            let _ = generations.scope_for_new_item();
        }
        for _ in 0..9 {
            generations.item_dropped(0);
        }
        assert_eq!(generations.len(), 1);

        generations.retire_if_needed(1);
        assert_eq!(generations.len(), 2, "the crowded generation was retired");
        assert_eq!(generations.current(), 1);
    }

    #[test]
    fn a_retired_generation_holding_live_items_keeps_its_scope() {
        install().ok();
        let root = Owner::new();
        let mut generations = Generations::new(root);
        for _ in 0..10 {
            let _ = generations.scope_for_new_item();
        }
        for _ in 0..9 {
            generations.item_dropped(0);
        }
        generations.retire_if_needed(1);

        assert_eq!(generations.live_generations(), 2);
        generations.item_dropped(0);
        assert_eq!(
            generations.live_generations(),
            1,
            "its last item went, so its scope went with it"
        );
    }

    #[test]
    fn a_new_generation_is_a_child_of_the_list_and_not_of_its_predecessor() {
        install().ok();
        let root = Owner::new();
        let mut generations = Generations::new(root.clone());
        let (_, first_item) = generations.scope_for_new_item();
        let first_depth = first_item.ancestry().len();

        for _ in 0..4 {
            let _ = generations.scope_for_new_item();
        }
        for _ in 0..5 {
            generations.item_dropped(0);
        }
        generations.retire_if_needed(0);
        let (_, second_item) = generations.scope_for_new_item();

        assert_eq!(
            second_item.ancestry().len(),
            first_depth,
            "a generation per retirement would deepen the scope tree without bound"
        );
    }

    #[test]
    fn the_current_generation_is_never_dropped_even_with_no_live_items() {
        install().ok();
        let root = Owner::new();
        let mut generations = Generations::new(root);
        let _ = generations.scope_for_new_item();
        generations.item_dropped(0);
        assert_eq!(generations.live_generations(), 1);
        // ... and it can still hand out scopes.
        let (at, _) = generations.scope_for_new_item();
        assert_eq!(at, 0);
    }
}
