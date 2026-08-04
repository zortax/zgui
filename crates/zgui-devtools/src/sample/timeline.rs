//! Where the last frame's time went, stage by stage.
//!
//! Built out of the marks the frame loop already writes, kept in memory rather than in a file. A
//! mark is an *instant*, not a duration, so every gap between two consecutive marks is visible
//! whether or not anybody thought to name what filled it — which is the property that matters here:
//! the stage that turns out to be expensive is usually the one nobody expected to have a name.
//!
//! The frame shown is the one *before* the frame being drawn. It has to be: the marks of the
//! current frame are still being written while the strip that would show them is being built, so a
//! strip of the current frame would always be a strip of half a frame.

/// One stage of a frame: what it was and how long it took.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Stage {
    /// The mark that opened it.
    pub(crate) name: String,
    /// What the mark had to say, truncated to something a strip can hold.
    pub(crate) note: String,
    /// How long until the next mark, in microseconds.
    pub(crate) us: f64,
}

/// How many marks are read to find the last complete frame.
///
/// The ring holds hundreds of frames, and every mark in it carries a heap-allocated note, so
/// reading all of it to draw one frame costs thousands of allocations per frame drawn. A frame
/// writes on the order of twenty marks, so this is a generous dozen frames' worth — enough that
/// the frame being drawn while this runs is never the one found, and small enough that the read is
/// a fixed cost whatever the ring's capacity is.
const WINDOW: usize = 256;

/// How many stages the strip will show.
///
/// A strip 420 px wide cannot legibly distinguish more than a few dozen slices, so building more
/// than this is work spent on something nobody can read. It is also the bound that matters: what
/// this returns becomes rows in a document, and rows are elements, and elements are what write the
/// marks this reads — so a sampler with no ceiling is a document whose size is a function of its
/// own size.
const STAGES: usize = 64;

/// The stages of the most recent complete frame in the ring.
///
/// Empty when no complete frame has been recorded yet, which is the state for exactly one frame
/// after the inspector is first opened — and also the answer for a span of marks with no frame
/// boundary in it, which is not a frame and must not be drawn as one.
///
/// At most [`STAGES`] of them, longest first by duration and then put back into the order they ran
/// in: a frame that wrote more boundaries than the strip can show is answered with where its time
/// actually went rather than with its first sixty-four microseconds.
pub(crate) fn sample_timeline() -> Vec<Stage> {
    stages().unwrap_or_default()
}

/// How many records are read to find the last complete frame's duration.
///
/// A frame writes on the order of thirty-five — marks *and* the notes between them — and this has
/// to reach back over the frame in progress to the whole of the one before it, which is two of
/// them. Sixty-four was not enough and failed in the way that is hardest to see: it found the last
/// `f.end` and not the `f.begin` belonging to it, so the chart stayed empty rather than wrong.
const SPAN: usize = 192;

/// How long the last complete frame took, in microseconds.
///
/// The same span the strip is built from, measured rather than itemised: the chart wants one number
/// per frame and building the stage list to add them up would be an allocation per stage for a
/// figure the two boundary marks already carry.
pub(crate) fn frame_total_us() -> Option<f64> {
    let marks = zgui_profile::latency::last(SPAN);
    let end = marks.iter().rposition(|mark| mark.stage == "f.end")?;
    let begin = marks[..end]
        .iter()
        .rposition(|mark| mark.stage == "f.begin")?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a frame is microseconds, so the nanosecond span is far inside f64"
    )]
    let us = marks[end].at_ns.saturating_sub(marks[begin].at_ns) as f64 / 1000.0;
    Some(us)
}

/// The stages, or nothing when the last [`WINDOW`] marks hold no complete frame.
fn stages() -> Option<Vec<Stage>> {
    let marks = zgui_profile::latency::last(WINDOW);
    // The last complete frame is the one bracketed by the last `f.begin` that has an `f.end` after
    // it. Walking back from the end rather than forward from the start is what keeps this cheap on
    // a ring that holds hundreds of frames.
    let end = marks.iter().rposition(|mark| mark.stage == "f.end")?;
    let begin = marks[..end]
        .iter()
        .rposition(|mark| mark.stage == "f.begin")?;
    let mut stages: Vec<(usize, Stage)> = marks[begin..=end]
        .windows(2)
        .map(|pair| Stage {
            name: pair[0].stage.to_owned(),
            note: pair[0].note.chars().take(72).collect(),
            #[expect(
                clippy::cast_precision_loss,
                reason = "a frame is microseconds, so the nanosecond gap is far inside f64"
            )]
            us: pair[1].at_ns.saturating_sub(pair[0].at_ns) as f64 / 1000.0,
        })
        .filter(|stage| stage.us > 0.0)
        .enumerate()
        .collect();
    if stages.len() > STAGES {
        stages.sort_by(|left, right| {
            right
                .1
                .us
                .partial_cmp(&left.1.us)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        stages.truncate(STAGES);
        stages.sort_by_key(|(at, _)| *at);
    }
    Some(stages.into_iter().map(|(_, stage)| stage).collect())
}

#[cfg(test)]
mod tests {
    use super::{STAGES, sample_timeline};

    /// Writes one frame of `marks` boundaries into the ring, spaced a microsecond apart.
    fn a_frame_of(marks: usize) {
        zgui_profile::latency::retain(8192);
        zgui_profile::latency::clear();
        zgui_profile::latency::mark("f.begin");
        for _ in 0..marks {
            std::thread::sleep(std::time::Duration::from_micros(1));
            zgui_profile::latency::mark("t.stage");
        }
        std::thread::sleep(std::time::Duration::from_micros(1));
        zgui_profile::latency::mark("f.end");
    }

    /// A frame that wrote more boundaries than the strip can show is still one strip.
    ///
    /// The bound that matters, because what this returns becomes rows in a document and rows are
    /// elements — and elements are what write the marks this reads. Without a ceiling here the
    /// document's size is a function of its own size, which is the runaway.
    #[test]
    fn a_frame_with_more_boundaries_than_the_strip_can_show_is_clamped() {
        a_frame_of(200);
        let stages = sample_timeline();
        assert!(
            stages.len() <= STAGES,
            "a 200-boundary frame produced {} stages",
            stages.len()
        );
        assert!(!stages.is_empty(), "it produced none at all");

        // And past the window there is no frame to find. A span of marks with no boundary in it is
        // not a frame, and drawing one as though it were is what used to turn a runaway into a
        // strip of thousands of slices.
        a_frame_of(2000);
        assert!(
            sample_timeline().is_empty(),
            "a span of marks with no frame boundary in it was drawn as a frame"
        );
        zgui_profile::latency::retain(0);
    }
}
