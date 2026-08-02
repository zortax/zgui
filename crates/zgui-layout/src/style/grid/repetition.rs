//! A `repeat()` clause, presented to the layout algorithms without being expanded.
//!
//! Expanding a repetition into its tracks is the algorithms' job, not this crate's: `auto-fill` and
//! `auto-fit` do not know how many times they repeat until the container has been measured. What
//! travels across is therefore the count, an iterator over the repeated tracks, and the line names
//! that accompany them.

use taffy::{GenericRepetition, RepetitionCount, TrackSizingFunction};
use zgui_css::values::grid::{RepeatCount, TrackRepeatValue};
use zgui_css::values::length::LengthPercentage;
use zgui_interned::Ident;

use crate::style::grid::names::EmptyLineNames;
use crate::style::grid::tracks::track;

/// One `repeat()` clause of one track list.
#[derive(Clone, Copy, Debug)]
pub struct Repetition<'a> {
    /// The clause itself.
    repeat: &'a TrackRepeatValue<LengthPercentage, i32>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl<'a> Repetition<'a> {
    /// Presents one clause.
    pub fn new(repeat: &'a TrackRepeatValue<LengthPercentage, i32>, scale: f32) -> Self {
        Self { repeat, scale }
    }
}

/// The tracks of one `repeat()` clause.
#[derive(Clone, Debug)]
pub struct RepetitionTracks<'a> {
    /// What is left to yield.
    sizes: core::slice::Iter<'a, zgui_css::values::grid::TrackSizeValue>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl Iterator for RepetitionTracks<'_> {
    type Item = TrackSizingFunction;

    fn next(&mut self) -> Option<TrackSizingFunction> {
        self.sizes.next().map(|size| track(size, self.scale))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sizes.size_hint()
    }
}

impl ExactSizeIterator for RepetitionTracks<'_> {}

impl GenericRepetition for Repetition<'_> {
    type CustomIdent = Ident;
    type RepetitionTrackList<'b>
        = RepetitionTracks<'b>
    where
        Self: 'b;
    type TemplateLineNames<'b>
        = EmptyLineNames<'b>
    where
        Self: 'b;

    fn count(&self) -> RepetitionCount {
        match &self.repeat.count {
            RepeatCount::Number(times) => {
                RepetitionCount::Count(u16::try_from(*times).unwrap_or(u16::MAX))
            }
            RepeatCount::AutoFill => RepetitionCount::AutoFill,
            RepeatCount::AutoFit => RepetitionCount::AutoFit,
        }
    }

    fn tracks(&self) -> Self::RepetitionTrackList<'_> {
        RepetitionTracks {
            sizes: self.repeat.track_sizes.iter(),
            scale: self.scale,
        }
    }

    fn lines_names(&self) -> Self::TemplateLineNames<'_> {
        // Names written inside a repetition are not carried: what a line inside an `auto-fill`
        // repetition is called depends on how many times it repeated, and the container names
        // reported outside cover every case a component library needs.
        crate::style::grid::names::no_line_names()
    }
}
