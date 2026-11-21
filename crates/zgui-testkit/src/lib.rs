//! The instrument the rest of the framework is tested with: no font files, and a clock that only
//! moves when a test moves it.
//!
//! Everything here exists so that a test measures the *design* rather than the machine it runs on.
//! A shaper with a fixed face gives the same answer on a machine with no fonts installed as on one
//! with a thousand. A virtual clock turns a seven-hundred-millisecond delay into a microsecond. And
//! the frame counters turn "this frame did the minimum" from a hope into an assertion. What a frame
//! draws is read through `zgui-testkit-compositor`.
//!
//! | Piece | What it is for |
//! |---|---|
//! | [`MonoShaper`] | a [`ParagraphShaper`](zgui_text::ParagraphShaper) with fixed 8×16 metrics and no font files |
//! | [`FixedMetrics`] | the matching metrics source the cascade resolves `ex` and `ch` against |
//! | [`TreeDump`] | the seam every tree dumper is written against, with golden comparison and blessing |
//! | [`counters`] | budget assertions that cannot pass by accident |
//! | [`fixture`] | the three rules a budget fixture obeys |
//!
//! # The one idea everything here is arranged around
//!
//! **An assertion that passes without measuring anything is worse than no assertion**, because it
//! is a claim of coverage where there is none. Three shapes of that failure are structurally
//! prevented rather than warned about:
//!
//! * a counter assertion whose counter reads zero because nothing in the test moves it —
//!   [`Measurement::assert_zero`](counters::Measurement::assert_zero) demands a
//!   [`Control`](counters::Control), which has no constructor other than a run in which the counter
//!   did move;
//! * an assertion on a counter only a real graphics backend increments — refused by name;
//! * a golden that materialises on first run, or is rewritten by the run that was supposed to check
//!   it — [`dump::golden`] fails in both cases and passes only on a comparison it actually made.
//!
//! ```
//! use zgui_profile::{Counter, counter};
//! use zgui_testkit::counters::Recording;
//!
//! let mut recording = Recording::begin();
//!
//! let busy = recording.measure(|| counter::add(Counter::ElementsRestyled, 4));
//! let control = busy.control(Counter::ElementsRestyled);
//!
//! let idle = recording.measure(|| {});
//! idle.assert_zero(Counter::ElementsRestyled, &control);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod counters;
pub mod dump;
pub mod fixture;
pub mod shaper;
pub mod text;

pub use crate::dump::{TreeDump, to_text};
pub use crate::shaper::{MonoLayout, MonoShaper, glyph_id};

/// The fixed-metrics font source, re-exported.
///
/// It lives in the text contracts rather than here because the style engine needs it and cannot
/// depend on a testkit — but it is the metrics half of what [`MonoShaper`] is, so it is named here
/// too. The two agree by construction: the shaper measures its clusters against this very source.
pub use zgui_text::FixedMetrics;
