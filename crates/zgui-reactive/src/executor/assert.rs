//! Debug guards for the two mistakes that otherwise fail in complete silence.

use reactive_graph::owner::Owner;

use crate::executor::ui_thread::is_ui_thread;

/// The owner depth at which a nesting mistake is more likely than a genuinely deep tree.
///
/// A component tree nests one owner per mounted node, so hundreds are ordinary and thousands
/// are not: a chain this long usually means owners are being created in a loop without ever
/// being disposed of.
const MAX_OWNER_DEPTH: usize = 4096;

/// The number of children one owner may accumulate before the growth is worth reporting.
///
/// The underlying engine never removes a child from its parent's list, so a parent that
/// outlives many short-lived children grows without bound unless the children are grouped
/// under a generation that is retired as a whole — which is what [`Scope`](crate::own::Scope)
/// does.
const MAX_OWNER_CHILDREN: usize = 1024;

/// Panics in debug builds if there is no current owner.
///
/// Providing a context, storing a value or constructing any handle the reactive arena backs is
/// a **silent no-op and a permanent leak** when no owner is current: nothing panics, nothing is
/// logged, and the value is never reachable or freed again. Any API with that property calls
/// this first, and so should any caller that builds one indirectly.
///
/// `context` names the operation, and appears in the panic message.
///
/// In release builds this compiles away to nothing.
#[track_caller]
pub fn assert_owner(context: &str) {
    if cfg!(debug_assertions) {
        assert!(
            Owner::current().is_some(),
            "{context} requires a current owner: with none, the value it creates is unreachable \
             and never freed. Run it inside `Mounted::with`."
        );
    }
}

/// Panics in debug builds if the calling thread does not run reactive work.
///
/// Reactive *state* is shared across threads; reactive *execution* is not. Polling tasks,
/// disposing of owners and reading anything held in a local context are only correct on the
/// thread that installed the runtime, and getting it wrong corrupts the task pool rather than
/// failing cleanly.
///
/// `context` names the operation, and appears in the panic message.
///
/// In release builds this compiles away to nothing.
#[track_caller]
pub fn assert_ui_thread(context: &str) {
    if cfg!(debug_assertions) {
        assert!(
            is_ui_thread(),
            "{context} must run on the thread that installed the reactive runtime"
        );
    }
}

/// Reports, in debug builds, an owner chain deep enough to indicate undisposed owners.
///
/// Called where a new owner is created. Reports rather than panics: a deep tree is legal, just
/// unlikely, and a frame that presents with a warning is better than one that aborts.
pub(crate) fn note_owner_depth(owner: &Owner) {
    if cfg!(debug_assertions) {
        let depth = owner.ancestry().len();
        if depth > MAX_OWNER_DEPTH {
            tracing::error!(
                depth,
                limit = MAX_OWNER_DEPTH,
                "owner chain is longer than expected; owners are probably being nested without \
                 being disposed of"
            );
        }
    }
}

/// Reports, in debug builds, a single owner accumulating more children than expected.
///
/// `count` is the number of children created under one owner that have not been discarded with
/// it. See [`Scope`](crate::own::Scope) for the retirement that keeps this bounded.
pub(crate) fn note_owner_children(count: usize) {
    if cfg!(debug_assertions) && count > MAX_OWNER_CHILDREN {
        tracing::error!(
            count,
            limit = MAX_OWNER_CHILDREN,
            "one owner has accumulated more children than expected; the generation holding them \
             is not being retired"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "requires a current owner")]
    fn assert_owner_panics_with_no_owner() {
        assert_owner("this test");
    }

    #[test]
    fn assert_owner_accepts_a_current_owner() {
        let owner = Owner::new();
        owner.with(|| assert_owner("this test"));
        owner.cleanup();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "installed the reactive runtime")]
    fn assert_ui_thread_panics_off_the_ui_thread() {
        assert_ui_thread("this test");
    }
}
