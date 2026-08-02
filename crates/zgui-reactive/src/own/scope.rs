//! Retiring generations of sibling scopes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use reactive_graph::owner::Owner;

use crate::executor::note_owner_children;
use crate::own::mounted::Mounted;

/// The fewest disposed members a generation must hold before it is worth replacing.
///
/// A floor under the "more dead than live" rule, so a scope holding one or two members at a time
/// replaces a generation every few dozen churns rather than on every unmount.
const RETIRE_AFTER: usize = 64;

/// A parent for a changing set of sibling scopes, such as the rows of a list.
///
/// An owner keeps a reference to every child ever created under it and removes none of them when
/// they are disposed of. A long-lived parent with short-lived children — a list that scrolls, a
/// table that filters, a route that changes — therefore grows one dead entry per child *ever
/// created*, and disposing of that parent eventually costs time proportional to everything it has
/// ever held rather than to what it holds now.
///
/// A scope fixes that by handing out members from a *generation*, and retiring generations in
/// place. New members are created under the newest generation; when that generation is holding
/// more dead members than the whole scope is holding live ones, a fresh generation is added
/// beside it and every existing member **stays exactly where it is**. A generation that is no
/// longer the newest and has no member left is dropped whole, at which point it has nothing to
/// deregister. Live members therefore never delay retirement, which matters because the case that
/// grows fastest — a virtualised list scrolling through a million rows — is also the case that
/// always has rows on screen.
///
/// Generations are siblings under the scope's own owner, never nested inside one another.
/// Nesting them would mean that dropping a spent generation disposed of the one that replaced it,
/// taking every live member with it.
///
/// What is left is one dead entry per *generation* rather than one per member, and the scope
/// reports both numbers: [`generation_children`](Scope::generation_children) is what would grow
/// without bound if generations were never retired, and
/// [`generations_created`](Scope::generations_created) is what still does grow, far more slowly.
///
/// ```
/// use zgui_reactive::{Scope, install};
///
/// install().unwrap();
/// let scope = Scope::new();
///
/// // One row stays on screen for the whole scroll, and does not hold the others' storage.
/// let pinned = scope.mount();
///
/// for _ in 0..10_000 {
///     let row = scope.mount();
///     // ... build the row ...
///     row.unmount();
/// }
///
/// assert_eq!(scope.live(), 1);
/// assert!(scope.generation_children() < 100);
/// pinned.unmount();
/// ```
#[derive(Debug)]
pub struct Scope {
    /// State shared with every scope mounted into it.
    shared: Rc<Membership>,
}

impl Scope {
    /// Creates a scope whose generations hang off the current owner.
    ///
    /// With no current owner, the scope is a root: it is freed when it is dropped, and not
    /// before.
    #[must_use]
    pub fn new() -> Self {
        let parent = Owner::current().unwrap_or_default();
        let generation = Generation {
            id: 0,
            owner: parent.child(),
            created: 0,
            live: 0,
        };
        Self {
            shared: Rc::new(Membership {
                parent,
                generations: RefCell::new(vec![generation]),
                next_id: Cell::new(1),
                live: Cell::new(0),
                created: Cell::new(1),
            }),
        }
    }

    /// Creates a scope for one member of this set.
    ///
    /// The member belongs to the newest generation, so disposing of the whole scope disposes of
    /// it, and disposing of it alone leaves its siblings untouched. Retiring a generation never
    /// moves it.
    ///
    /// # Panics
    ///
    /// In debug builds, if called off the UI thread.
    #[track_caller]
    pub fn mount(&self) -> Mounted {
        let (owner, generation) = {
            let mut generations = self.shared.generations.borrow_mut();
            let current = generations
                .last_mut()
                .expect("a live scope always holds a generation");
            let owner = current.owner.child();
            current.created += 1;
            current.live += 1;
            note_owner_children(current.created);
            (owner, current.id)
        };
        self.shared.live.set(self.shared.live.get() + 1);
        Mounted::from_owner(
            owner,
            Some(Member {
                shared: Rc::clone(&self.shared),
                generation,
            }),
        )
    }

    /// How many members are mounted right now.
    #[must_use]
    pub fn live(&self) -> usize {
        self.shared.live.get()
    }

    /// How many members the newest generation has ever held.
    ///
    /// The number that would grow without bound if generations were never retired. Diagnostic:
    /// it is exact, but what counts as "too many" is a policy this type owns.
    #[must_use]
    pub fn generation_children(&self) -> usize {
        self.shared
            .generations
            .borrow()
            .last()
            .map_or(0, |generation| generation.created)
    }

    /// How many generations this scope has ever created.
    ///
    /// One dead entry is left behind in the scope's own owner per generation, so this is the
    /// residual growth retirement trades the per-member growth for. It rises by one per
    /// [`generation_children`](Scope::generation_children)-worth of churn rather than once per
    /// member.
    #[must_use]
    pub fn generations_created(&self) -> usize {
        self.shared.created.get()
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Scope {
    /// Disposes of every generation, and with it every member still mounted.
    fn drop(&mut self) {
        // Taken before disposing, so a cleanup that drops one of this scope's own members finds
        // the bookkeeping readable rather than already borrowed.
        let generations = std::mem::take(&mut *self.shared.generations.borrow_mut());
        for generation in &generations {
            generation.owner.cleanup();
        }
    }
}

/// One replaceable parent for a run of members.
#[derive(Debug)]
struct Generation {
    /// Distinguishes this generation from the ones before and after it.
    id: u64,
    /// The owner members of this generation are created under.
    owner: Owner,
    /// Members ever created in it.
    created: usize,
    /// Members not yet disposed of.
    live: usize,
}

/// One member's link back to the scope that handed it out.
#[derive(Debug)]
pub(crate) struct Member {
    /// The scope's shared bookkeeping.
    shared: Rc<Membership>,
    /// The generation this member was created in, which never changes.
    generation: u64,
}

impl Member {
    /// Records that this member has been disposed of.
    pub(crate) fn released(&self) {
        self.shared.released(self.generation);
    }
}

/// The state a [`Scope`] shares with the [`Mounted`] scopes it handed out.
#[derive(Debug)]
struct Membership {
    /// The owner the generations hang off. Never a generation itself.
    parent: Owner,
    /// The generations, oldest first; the last one is where new members are created.
    generations: RefCell<Vec<Generation>>,
    /// The id the next generation will take.
    next_id: Cell<u64>,
    /// Members not yet disposed of, across every generation.
    live: Cell<usize>,
    /// Generations ever created.
    created: Cell<usize>,
}

impl Membership {
    /// Records that a member of `generation` was disposed of.
    ///
    /// Adds a generation when the newest one is holding more dead members than the scope is
    /// holding live ones, and drops every generation that is neither the newest nor still
    /// occupied.
    fn released(&self, generation: u64) {
        self.live.set(self.live.get().saturating_sub(1));

        // Retired generations are dropped after the bookkeeping is given back, so the drop cannot
        // re-enter it.
        let mut retired = Vec::new();
        {
            // Borrowed rather than taken, because the scope may be disposing of everything it
            // holds right now — in which case there is no bookkeeping left to update.
            let Ok(mut generations) = self.generations.try_borrow_mut() else {
                return;
            };
            if generations.is_empty() {
                return;
            }
            if let Some(member) = generations.iter_mut().find(|held| held.id == generation) {
                member.live = member.live.saturating_sub(1);
            }

            let current = generations
                .last()
                .expect("the emptiness check above already returned");
            if current.created.saturating_sub(current.live) > self.live.get().max(RETIRE_AFTER) {
                let id = self.next_id.get();
                self.next_id.set(id + 1);
                self.created.set(self.created.get() + 1);
                // A sibling of its predecessor, never a child of it: chaining them would make
                // dropping a spent generation dispose of the one that replaced it.
                generations.push(Generation {
                    id,
                    owner: self.parent.child(),
                    created: 0,
                    live: 0,
                });
            }

            let newest = generations.len() - 1;
            let mut kept = Vec::with_capacity(generations.len());
            for (index, held) in generations.drain(..).enumerate() {
                if index == newest || held.live > 0 {
                    kept.push(held);
                } else {
                    retired.push(held);
                }
            }
            *generations = kept;
        }
        // Every member of a retired generation is already disposed of, so its owner finds only
        // dead references and cleans nothing.
        drop(retired);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use reactive_graph::signal::RwSignal;
    use reactive_graph::traits::GetUntracked;

    use super::*;
    use crate::executor::install;
    use crate::own::on_cleanup_local;

    #[test]
    fn a_churning_scope_does_not_accumulate_children() {
        install().unwrap();
        let scope = Scope::new();
        for _ in 0..10_000 {
            scope.mount().unmount();
        }
        assert_eq!(scope.live(), 0);
        assert!(
            scope.generation_children() <= RETIRE_AFTER + 1,
            "the generation was retired: {} children",
            scope.generation_children()
        );
    }

    #[test]
    fn a_live_member_does_not_delay_retirement_and_survives_it() {
        install().unwrap();
        let scope = Scope::new();
        let disposed = Rc::new(Cell::new(0));

        let kept = scope.mount();
        let value = kept.with({
            let disposed = Rc::clone(&disposed);
            move || {
                on_cleanup_local(move || disposed.set(disposed.get() + 1));
                RwSignal::new(7)
            }
        });

        for _ in 0..10_000 {
            scope.mount().unmount();
        }

        assert_eq!(
            disposed.get(),
            0,
            "the surviving member was not disposed of"
        );
        assert_eq!(scope.live(), 1);
        assert_eq!(
            value.get_untracked(),
            7,
            "and its storage outlived every retirement"
        );
        assert!(
            scope.generation_children() <= RETIRE_AFTER + 1,
            "one live member did not hold the generation open: {} children",
            scope.generation_children()
        );

        kept.unmount();
        assert_eq!(disposed.get(), 1);
    }

    #[test]
    fn retiring_the_generation_a_member_is_in_leaves_later_members_alone() {
        install().unwrap();
        let scope = Scope::new();

        let old = scope.mount();
        for _ in 0..=RETIRE_AFTER {
            scope.mount().unmount();
        }
        assert!(
            scope.generations_created() > 1,
            "a generation was added beside the one holding the live member"
        );

        let new = scope.mount();
        let value = new.with(|| RwSignal::new(3));

        // Disposing of the last member of the older generation drops that generation's owner.
        old.unmount();
        assert_eq!(
            value.get_untracked(),
            3,
            "the newer generation's members are untouched"
        );
        new.unmount();
    }

    #[test]
    fn dropping_a_scope_disposes_of_its_members() {
        install().unwrap();
        let disposed = Rc::new(Cell::new(0));
        let scope = Scope::new();
        let member = scope.mount();
        member.with({
            let disposed = Rc::clone(&disposed);
            move || on_cleanup_local(move || disposed.set(disposed.get() + 1))
        });
        std::mem::forget(member);

        drop(scope);
        assert_eq!(disposed.get(), 1);
    }

    #[test]
    fn dropping_a_scope_from_a_members_own_cleanup_does_not_panic() {
        install().unwrap();
        let scope = Scope::new();
        let first = scope.mount();
        let second = scope.mount();
        // The first member's cleanup disposes of a sibling, which asks the scope to record a
        // release while the scope is already disposing of everything it holds.
        first.with(move || on_cleanup_local(move || second.unmount()));

        drop(scope);
    }
}
