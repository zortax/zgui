//! The tasks one owner is responsible for.
//!
//! A task is cancelled when the scope that spawned it goes away, and the obvious way to arrange
//! that — one [`on_cleanup_local`](crate::on_cleanup_local) per spawn — is wrong here. A listener
//! runs inside the owner its `on:` binding was written in, because the view layer captures that
//! owner once and re-enters it on every dispatch. So a button that spawns on every click would
//! register one cleanup closure per click on a component owner that lives as long as the screen
//! does. That is the same unbounded growth [`Scope`](crate::Scope) exists to cure, arriving by a
//! different road.
//!
//! So the registration is per *owner*: the first spawn beneath an owner installs one set and one
//! cleanup, and every later spawn joins the set. A task leaves the set as soon as it is over, so a
//! handler that fires ten thousand times leaves nothing behind.

use std::cell::RefCell;
use std::rc::Rc;

use reactive_graph::owner::Owner;

use crate::context::{provide_local_context, use_local_context};
use crate::own::on_cleanup_local;
use crate::task::cancel::Cancel;

/// The slots of one set, and the ones no task is using.
///
/// A slab rather than a vector that is periodically swept, because the alternative leaves a set
/// holding every task it has ever run until the *next* spawn compacts it — and a screen that
/// loads once and then sits still never gets a next spawn. Here a task leaves the set the moment
/// it is over, at the cost of one `usize` per task.
#[derive(Default)]
struct Slots {
    /// The tasks, by index. A `None` is a slot waiting to be reused.
    entries: Vec<Option<Rc<Cancel>>>,
    /// Which indices are free, most recently freed first.
    free: Vec<usize>,
}

/// The live tasks of one owner, cancelled together when that owner is disposed of.
///
/// Held in the owner's local context, so every scope beneath it finds the same set. Cloning is
/// cloning the handle: there is one set per owner however many clones exist.
#[derive(Clone, Default)]
pub(crate) struct TaskSet {
    /// The slots. `Rc` because the release hook each task holds must not keep the set alive, and
    /// `Weak` is how it does not.
    slots: Rc<RefCell<Slots>>,
}

impl TaskSet {
    /// Adds `task` to the set, and arranges for it to leave when it is over.
    pub(crate) fn insert(&self, task: Rc<Cancel>) {
        let index = {
            let mut slots = self.slots.borrow_mut();
            match slots.free.pop() {
                Some(index) => {
                    slots.entries[index] = Some(Rc::clone(&task));
                    index
                }
                None => {
                    slots.entries.push(Some(Rc::clone(&task)));
                    slots.entries.len() - 1
                }
            }
        };

        // Weak, so a task that outlives its set — one cancelled by the very disposal that dropped
        // the set — releases into nothing instead of resurrecting it.
        let slots = Rc::downgrade(&self.slots);
        task.on_release(move || {
            let Some(slots) = slots.upgrade() else { return };
            // Already borrowed means the set is being cancelled wholesale and has taken its slots
            // already; there is nothing left to remove this from.
            let Ok(mut slots) = slots.try_borrow_mut() else {
                return;
            };
            if let Some(slot) = slots.entries.get_mut(index)
                && slot.take().is_some()
            {
                slots.free.push(index);
            }
            // A set that has run a great many tasks and is now holding none gives its slots back,
            // so the high-water mark of a burst is not carried for the life of the component.
            if slots.free.len() == slots.entries.len() {
                slots.entries.clear();
                slots.free.clear();
            }
        });
    }

    /// Cancels every task in the set.
    ///
    /// The slots are taken before any task is cancelled, so a task whose captures spawn something
    /// as they drop is added to a fresh set rather than to the one being walked.
    pub(crate) fn cancel_all(&self) {
        let taken = std::mem::take(&mut *self.slots.borrow_mut());
        for task in taken.entries.into_iter().flatten() {
            task.cancel();
        }
    }

    /// How many tasks the set is holding. For tests.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        let slots = self.slots.borrow();
        slots.entries.len() - slots.free.len()
    }
}

/// The set the current scope spawns into, installing one if this is the first spawn beneath it.
///
/// Returns `None` when there is no current owner, which is what makes an ownerless spawn detached
/// rather than a silent leak of a set nothing will ever dispose of.
///
/// Where the set lands matters, and callers with a say in it should exercise it. A set is
/// installed into whichever owner happens to be current at the first spawn, so the framework
/// installs one eagerly at the boundaries that ought to own tasks — a component and a window —
/// and lets everything below join it. Without that, the first spawn inside a short-lived effect
/// scope would put the set there, and every task the component spawned afterwards would be
/// cancelled the next time that effect re-ran.
pub(crate) fn current_or_install() -> Option<TaskSet> {
    if let Some(set) = use_local_context::<TaskSet>() {
        return Some(set);
    }
    // Held only for the length of this call: the question is whether there is an owner to attach a
    // set to, and the answer is a clone of the handle rather than the owner itself.
    let _current = Owner::current()?;
    Some(install())
}

/// Installs a fresh set into the current owner, replacing any the owner itself provided.
///
/// Called at the boundaries that should own their tasks. Providing a set in a scope that already
/// sees one from above is the point: it shadows the outer set for this subtree, so a component's
/// tasks die with the component rather than with its parent.
///
/// # Panics
///
/// In debug builds, if called off the UI thread or with no current owner.
#[track_caller]
pub fn provide_task_set() {
    install();
}

/// Creates a set, provides it, and arranges for it to be cancelled with the current owner.
#[track_caller]
fn install() -> TaskSet {
    let set = TaskSet::default();
    provide_local_context(set.clone());
    on_cleanup_local({
        let set = set.clone();
        move || set.cancel_all()
    });
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::install as install_runtime;

    #[test]
    fn one_owner_holds_one_set_however_many_spawns_join_it() {
        install_runtime().unwrap();
        let owner = Owner::new();

        owner.with(|| {
            let first = current_or_install().expect("an owner is current");
            let second = current_or_install().expect("an owner is current");
            first.insert(Cancel::new(async {}));
            assert_eq!(second.len(), 1, "the second spawn joined the first's set");
        });

        owner.cleanup();
    }

    #[test]
    fn a_task_that_is_over_leaves_the_set_at_once() {
        install_runtime().unwrap();
        let owner = Owner::new();

        owner.with(|| {
            let set = current_or_install().expect("an owner is current");
            let task = Cancel::new(std::future::pending());
            set.insert(Rc::clone(&task));
            assert_eq!(set.len(), 1);

            task.cancel();
            assert_eq!(
                set.len(),
                0,
                "it took itself out rather than waiting to be swept"
            );
        });

        owner.cleanup();
    }

    #[test]
    fn a_set_that_has_run_thousands_of_tasks_holds_none_of_them() {
        install_runtime().unwrap();
        let owner = Owner::new();

        owner.with(|| {
            let set = current_or_install().expect("an owner is current");
            for _ in 0..10_000 {
                let task = Cancel::new(async {});
                set.insert(Rc::clone(&task));
                task.cancel(); // stands in for "it ran to completion"
            }
            assert_eq!(set.len(), 0, "the set is still holding finished tasks");
        });

        owner.cleanup();
    }

    #[test]
    fn a_task_already_over_when_it_is_inserted_does_not_stay() {
        install_runtime().unwrap();
        let owner = Owner::new();

        owner.with(|| {
            let set = current_or_install().expect("an owner is current");
            let task = Cancel::new(async {});
            task.cancel();
            set.insert(task);
            assert_eq!(set.len(), 0);
        });

        owner.cleanup();
    }

    #[test]
    fn disposing_of_the_owner_cancels_everything_in_its_set() {
        install_runtime().unwrap();
        let owner = Owner::new();
        let task = owner.with(|| {
            let set = current_or_install().expect("an owner is current");
            let task = Cancel::new(std::future::pending());
            set.insert(Rc::clone(&task));
            task
        });

        assert!(!task.is_over());
        owner.cleanup();
        assert!(task.is_over(), "the unmount cancelled it");
    }

    #[test]
    fn a_provided_set_shadows_the_one_above_it() {
        install_runtime().unwrap();
        let parent = Owner::new();

        parent.with(|| {
            let outer = current_or_install().expect("an owner is current");
            let child = Owner::new();
            let inner = child.with(|| {
                provide_task_set();
                let set = current_or_install().expect("an owner is current");
                set.insert(Cancel::new(std::future::pending()));
                set
            });

            assert_eq!(inner.len(), 1);
            assert_eq!(outer.len(), 0, "the child's task did not join its parent's");
            child.cleanup();
        });

        parent.cleanup();
    }

    #[test]
    fn there_is_no_set_without_an_owner() {
        install_runtime().unwrap();
        assert!(current_or_install().is_none());
    }
}
