//! The assertion a counter of avoided work is worthless without.
//!
//! A counter of work *performed* is falsifiable on its own: it reads zero when the stage did
//! nothing and a number when it did something. A counter of work *avoided* is not. It reads zero
//! when the stage is perfect, zero when the stage has been deleted, and zero when nobody ever
//! wired it up — so `skipped <= n` is green from the day it is written whatever happens to the
//! code underneath it.
//!
//! [`assert_non_vacuous`] is what makes such a counter mean something: it drives the situation the
//! skip exists for and requires the counter to move, then drives one in which skipping would be
//! wrong and requires it to stay exactly where it was.

mod scenario;

use crate::counter::exclusive::exclusive;
use crate::counter::store::{COUNTERS_ENABLED, snapshot};
use crate::counter::table::Counter;

pub use crate::counter::non_vacuity::scenario::Scenario;

/// Proves that `skipped` can be moved and can be left alone, by driving both.
///
/// `fires` is the situation the skip exists for and must move the counter. `silent` is a situation
/// in which skipping would be wrong and must leave it exactly where it was. Both are required, and
/// so is a third thing neither of them states: `silent` must move the counter of work *performed*,
/// or "nothing was skipped" would be satisfied by a scenario in which the stage was never asked to
/// do anything at all.
///
/// `skipped` must be declared [`Group::Skip`](crate::Group::Skip), which is where the counter of
/// work performed comes from. `cargo xtask skips` searches the workspace's test targets for calls
/// to this function and fails the build for any skip counter that has none.
///
/// The counter block is process-wide, so this holds [`exclusive`](crate::counter::exclusive) for
/// the length of both scenarios. Two of these can be written in one test binary and will take
/// turns; a caller must not already be holding that guard, because it is not reentrant.
///
/// ```no_run
/// use zgui_profile::Counter;
/// use zgui_profile::counter::non_vacuity::{Scenario, assert_non_vacuous};
///
/// # fn scroll_a_painted_document() {}
/// # fn open_a_fresh_document() {}
/// assert_non_vacuous(
///     Counter::ChunksTranslated,
///     Scenario::new("scrolling a document already painted once", scroll_a_painted_document),
///     Scenario::new("the first paint of a fresh document", open_a_fresh_document),
/// );
/// ```
///
/// # Panics
///
/// Panics when `skipped` is not declared a skip, when `fires` did not move it, when `silent` moved
/// it, or when `silent` left the counter of work performed at zero. Each message names both
/// counters, the scenario that produced it, and what to change.
pub fn assert_non_vacuous(skipped: Counter, fires: Scenario<'_>, silent: Scenario<'_>) {
    let Some(done) = skipped.group().done() else {
        panic!(
            "`{}` is not declared as a skip, so there is no counter of work performed to read it \
             against. Declare it `Group::Skip {{ done: Counter::… }}` in the counter table, naming \
             the counter of the work the same stage does when it cannot reuse an answer.",
            skipped.name(),
        );
    };

    let _turn = exclusive();
    let fired = run(fires, skipped, done);
    let quiet = run(silent, skipped, done);
    if !COUNTERS_ENABLED {
        // The block is compiled out of this build, so every delta above is zero and every
        // assertion below would hold while measuring nothing. Returning is the honest outcome.
        return;
    }

    assert!(
        fired.skipped > 0,
        "`{skip}` stayed at zero over `{at}`, which is the situation the skip exists for. The \
         stage performed {done_count} unit(s) of work there and reused nothing — so either it is \
         no longer skipping, or the counter is not being incremented. Either way every budget \
         written against `{skip}` is passing without measuring anything.",
        skip = skipped.name(),
        at = fired.described,
        done_count = fired.done,
    );
    assert!(
        quiet.done > 0,
        "`{done}` stayed at zero over `{at}`, so the stage was never reached there and \
         `{skip} == 0` would hold over a scenario that does nothing at all. Drive something that \
         reaches the stage, or name a different scenario.",
        done = done.name(),
        at = quiet.described,
        skip = skipped.name(),
    );
    assert_eq!(
        quiet.skipped,
        0,
        "`{skip}` moved by {moved} over `{at}`, where nothing may be reused: the stage performed \
         {done_count} unit(s) of work and claimed to have skipped {moved} more. Either that \
         scenario is not the fresh one it is described as, or the stage is reusing an answer it \
         has no right to.",
        skip = skipped.name(),
        moved = quiet.skipped,
        at = quiet.described,
        done_count = quiet.done,
    );
}

/// What one scenario moved.
struct Moved {
    /// How the scenario was described at the call site.
    described: String,
    /// The skip counter's delta.
    skipped: u64,
    /// The performed-work counter's delta.
    done: u64,
}

/// Drives one scenario and reports what the pair moved by.
fn run(mut scenario: Scenario<'_>, skipped: Counter, done: Counter) -> Moved {
    let described = scenario.described().to_owned();
    let before = snapshot();
    scenario.drive();
    let after = snapshot();
    let delta = before.delta(&after);
    Moved {
        described,
        skipped: delta.get(skipped),
        done: delta.get(done),
    }
}

#[cfg(test)]
mod tests;
