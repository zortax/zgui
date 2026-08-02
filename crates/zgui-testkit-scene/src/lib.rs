//! The instrument the rest of the framework is tested with: no graphics device, no font files, and
//! a clock that only moves when a test moves it.
//!
//! Everything here exists so that a test measures the *design* rather than the machine it runs on.
//! A display list is a value, so it can be captured as text and diffed in a review. A shaper with a
//! fixed face gives the same answer on a machine with no fonts installed as on one with a thousand.
//! A virtual clock turns a seven-hundred-millisecond delay into a microsecond. And the frame
//! counters turn "this frame did the minimum" from a hope into an assertion.
//!
//! | Piece | What it is for |
//! |---|---|
//! | [`CaptureRenderer`] | a [`Renderer`](zgui_render::Renderer) that records the scene as stable text and draws nothing |
//! | [`MonoShaper`] | a [`ParagraphShaper`](zgui_text::ParagraphShaper) with fixed 8×16 metrics and no font files |
//! | [`FixedMetrics`] | the matching metrics source the cascade resolves `ex` and `ch` against |
//! | [`TreeDump`] | the seam every tree dumper is written against, with golden comparison and blessing |
//! | [`Harness`] | the frame loop, its virtual clock, its parking and its counters |
//! | [`counters`] | budget assertions that cannot pass by accident |
//! | [`fixture`] | what a harness is built from, and the three rules a budget fixture obeys |
//!
//! # The one idea everything here is arranged around
//!
//! **An assertion that passes without measuring anything is worse than no assertion**, because it
//! is a claim of coverage where there is none. Three shapes of that failure are structurally
//! prevented rather than warned about:
//!
//! * a counter assertion whose counter reads zero because nothing in the harness moves it —
//!   [`Measurement::assert_zero`](counters::Measurement::assert_zero) demands a
//!   [`Control`](counters::Control), which has no constructor other than a run in which the counter
//!   did move;
//! * an assertion on a counter only a real graphics backend increments — refused by name;
//! * a golden that materialises on first run, or is rewritten by the run that was supposed to check
//!   it — [`dump::golden`] fails in both cases and passes only on a comparison it actually made.
//!
//! ```
//! use zgui_profile::{Counter, counter};
//! use zgui_testkit_scene::counters::Recording;
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

pub mod capture;
pub mod counters;
pub mod dump;
pub mod fixture;
pub mod harness;
pub mod shaper;
pub mod text;
pub mod transcript;

pub use crate::capture::CaptureRenderer;
pub use crate::dump::{TreeDump, to_text};
pub use crate::fixture::Fixture;
pub use crate::harness::Harness;
pub use crate::harness::frame::{FrameCx, Pipeline};
pub use crate::shaper::{MonoLayout, MonoRaster, MonoShaper, glyph_id};
pub use crate::transcript::Transcript;

/// The clock a test moves by hand, re-exported.
///
/// It lives with the platform contract rather than here because a backend with no windowing system
/// behind it runs on one too — but it is the clock [`Harness`] advances, so it is named here as
/// well. There is exactly one implementation, so a frame run by hand and a frame run by that
/// backend cannot disagree about what time it is.
pub use zgui_platform::VirtualClock;

/// The fixed-metrics font source, re-exported.
///
/// It lives in the text contracts rather than here because the style engine needs it and cannot
/// depend on a testkit — but it is the metrics half of what [`MonoShaper`] is, so it is named here
/// too. The two agree by construction: the shaper measures its clusters against this very source.
pub use zgui_text::FixedMetrics;
