//! Exclusive access to the one process-wide counter block.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};

use zgui_profile::{COUNTERS_ENABLED, Counters, counter};

use crate::counters::measurement::Measurement;

/// Held for as long as one measurement is in progress.
static SERIAL: Mutex<()> = Mutex::new(());

thread_local! {
    /// Whether this thread already holds a recording, so reentrancy is a panic and not a deadlock.
    static HELD: Cell<bool> = const { Cell::new(false) };
}

/// Exclusive use of the counters, for as long as it is alive.
///
/// The counters are one block of atomics shared by the whole process, which is what makes writing
/// to them cheap enough to do anywhere. It also means two measurements taken at once measure each
/// other: a test asserting that a frame restyled nothing would read a restyle another test's frame
/// performed, and the failure would be intermittent and blamed on the wrong code. So a measurement
/// takes this, and a second one waits.
///
/// Dropping it leaves the counters at zero, so the next holder starts from a known state whether or
/// not this one finished tidily.
#[derive(Debug)]
pub struct Recording {
    /// Exclusive access, released on drop.
    _guard: MutexGuard<'static, ()>,
}

impl Recording {
    /// Takes the counters, resetting them.
    ///
    /// Blocks while another thread is measuring.
    ///
    /// # Panics
    ///
    /// Panics when the calling thread already holds a recording, which would otherwise deadlock
    /// against itself.
    ///
    /// A build in which the counters were compiled out is refused at *compile* time rather than
    /// here: every counter would read zero and every budget assertion written against them would
    /// hold while measuring nothing, so it must not be possible to reach this function in such a
    /// build at all. This crate's own manifest is what guarantees it.
    pub fn begin() -> Self {
        const {
            assert!(
                COUNTERS_ENABLED,
                "the frame counters are compiled out of this build, so every counter would read \
                 zero and every budget assertion would hold without measuring anything. This \
                 crate depends on `zgui-profile` with its `counters` feature on precisely so that \
                 this cannot happen."
            );
        }
        HELD.with(|held| {
            assert!(
                !held.get(),
                "this thread already holds a counter recording. Two recordings measure each other, \
                 so the second would have to wait for the first — which, on one thread, is a \
                 deadlock. Hold exactly one."
            );
            held.set(true);
        });
        // A test that panics while measuring poisons the lock. The counters carry no invariant a
        // panic could break — they are reset on the way in — so the guard is taken back rather than
        // failing every later test with a poisoning nobody can act on.
        let guard = SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        counter::reset();
        Self { _guard: guard }
    }

    /// Sets every counter back to zero.
    pub fn reset(&mut self) {
        counter::reset();
    }

    /// Every counter's value right now.
    pub fn snapshot(&self) -> Counters {
        counter::snapshot()
    }

    /// Resets the counters, runs `exercise`, and reports what it moved.
    pub fn measure(&mut self, exercise: impl FnOnce()) -> Measurement {
        self.reset();
        exercise();
        Measurement::new(self.snapshot())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        counter::reset();
        HELD.with(|held| held.set(false));
    }
}

#[cfg(test)]
mod tests {
    use zgui_profile::{Counter, counter};

    use super::Recording;

    #[test]
    fn a_measurement_reports_exactly_what_its_run_moved() {
        let mut recording = Recording::begin();
        let first = recording.measure(|| counter::add(Counter::PrimitivesEmitted, 3));
        assert_eq!(first.get(Counter::PrimitivesEmitted), 3);

        // The reset is part of `measure`, so the second run does not inherit the first's work.
        let second = recording.measure(|| counter::add(Counter::PrimitivesEmitted, 1));
        assert_eq!(second.get(Counter::PrimitivesEmitted), 1);
    }

    #[test]
    fn dropping_a_recording_leaves_the_counters_at_zero() {
        {
            let mut recording = Recording::begin();
            recording.measure(|| counter::bump(Counter::Repaints));
        }
        let recording = Recording::begin();
        assert_eq!(recording.snapshot().repaints, 0);
    }

    #[test]
    #[should_panic(expected = "already holds a counter recording")]
    fn a_second_recording_on_one_thread_is_a_panic_and_not_a_deadlock() {
        let _first = Recording::begin();
        let _second = Recording::begin();
    }
}
