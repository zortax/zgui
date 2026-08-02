//! Counter assertions that cannot pass by accident.
//!
//! A budget test says a frame did a certain amount of work. Almost all of the value is in the
//! assertions that a counter stayed *low* — "hovering one row of a thousand restyles the row and
//! its ancestors and nothing else" — and almost all of the ways such a test goes wrong end with it
//! passing while measuring nothing at all. Three of them are structural, and each is closed here
//! rather than left to a reviewer to spot:
//!
//! 1. **The counters were compiled out.** Every counter then reads zero and every upper bound
//!    holds. [`Recording::begin`] refuses to start unless
//!    [`COUNTERS_ENABLED`](zgui_profile::COUNTERS_ENABLED) is true.
//! 2. **The counter belongs to the renderer.** Under a capture renderer `draw_calls`, `damage_px`
//!    and `bytes_uploaded` read zero because nothing drew. [`Measurement::get`] refuses them by
//!    name, and every whole-snapshot read hands them back as [`POISON`] rather than as that zero,
//!    so a test that asserts on one fails loudly by either route instead of passing silently.
//! 3. **The mechanism never ran.** `elements_restyled == 0` is equally true of a pipeline that
//!    restyled precisely nothing and of one that has no restyle stage at all. So an assertion that
//!    a counter stayed at zero — or under a bound — requires a [`Control`]: a second, *deliberately
//!    different* run in which the same counter moved. A `Control` has no public constructor; the
//!    only way to obtain one is [`Measurement::control`], which panics when the counter it is asked
//!    for did not move.
//!
//! # The shape of a budget test
//!
//! ```
//! use zgui_profile::{Counter, counter};
//! use zgui_testkit_scene::counters::Recording;
//!
//! let mut recording = Recording::begin();
//!
//! // The control: a run that does the work, proving the counter can move at all.
//! let busy = recording.measure(|| counter::add(Counter::ElementsRestyled, 12));
//! let control = busy.control(Counter::ElementsRestyled);
//!
//! // The subject: the run under test, which must not do it.
//! let idle = recording.measure(|| {});
//! idle.assert_zero(Counter::ElementsRestyled, &control);
//! ```
//!
//! # The counters are one global block, so recordings are serialised
//!
//! The counters live in one process-wide block, which is what makes them cheap enough to write
//! anywhere. A test harness therefore holds a [`Recording`] for as long as it is measuring, and two
//! recordings cannot exist at once: without that, two tests running in parallel in one process
//! would each read the other's work and both assertions would be about nothing.
//!
//! **That makes it a rule about the whole test binary and not about each test.** A recording
//! excludes other *recordings*; it cannot stop an unguarded test in another thread from doing work
//! that moves a counter. So measuring tests go in a target where **every** test either holds a
//! [`Recording`] or holds a [`Harness`](crate::Harness), which holds one for its whole life. Cargo
//! runs test targets one at a time, so such a target measures exactly what it says.
//!
//! Documentation examples are one such target, and they are merged into one binary: an example that
//! measures a counter another example moves unguarded would be intermittently wrong. So the examples
//! here measure a counter no unguarded example in this crate touches.

pub mod control;
pub mod meaning;
pub mod measurement;
pub mod recording;

pub use crate::counters::control::Control;
pub use crate::counters::meaning::{POISON, RENDERER_SPECIFIC, is_meaningful};
pub use crate::counters::measurement::Measurement;
pub use crate::counters::recording::Recording;
