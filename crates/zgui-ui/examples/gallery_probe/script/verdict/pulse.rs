//! What a skeleton's pulse puts on the screen over one period of itself.
//!
//! The pulse moves a fill colour on a sixteen-hundred-millisecond keyframe, so a run of captures
//! taken as fast as one can be taken covers a period several times over. What each is judged on is
//! the average of the block, over the block's own rectangle: a fade that steps up, back down and up
//! again shows as a reversal in that series, and a fade that has stopped shows as no swing at all.
//!
//! The strip is also recorded with the animation count the engine reports for the block, so a series
//! that turns out to be flat is separable into a pulse that is not running and a pulse that is
//! running and not reaching the picture.

use core::time::Duration;

use crate::script::find;
use crate::script::gauntlet::ink::shot_of;
use crate::script::verdict;
use crate::stage::Stage;

/// How many pictures to take.
const SAMPLES: usize = 20;

/// How long to leave between two captures, on top of what a capture costs.
const APART: Duration = Duration::from_millis(20);

/// How much room to leave around the row, in device pixels.
const MARGIN: f32 = 4.0;

/// The tallest a single block can be, in device pixels.
///
/// The gallery asks for sixteen CSS pixels, and this leaves room for that at any scale a desktop is
/// likely to report while still being shorter than the column the three of them are in.
const TALLEST: f32 = 40.0;

/// Photographs the pulsing blocks.
pub(crate) fn run(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Avatar and skeleton") else {
        stage
            .report
            .note("Skeleton", "the skeleton panel is not laid out");
        return;
    };
    let Some(row) = verdict::row_of(&census, panel, "skeleton") else {
        stage
            .report
            .note("Skeleton", "the row of skeletons is not laid out");
        return;
    };
    stage.leave();

    // The widest silent block in the row that is no taller than a block, which is the middle
    // skeleton: it has no width of its own in the sheet and so spans the column, and it is the one
    // whose average is least disturbed by the card showing beside it. The height is what tells a
    // block from the column holding all three, which is silent and wider still.
    let block = census
        .inside(row)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0)
        .filter(|node| node.rect.is_some_and(|rect| rect.size.height.0 <= TALLEST))
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect.map(|rect| (node.id, rect)));
    let Some((node, rect)) = block else {
        stage
            .report
            .note("Skeleton", "no block in the skeleton row is laid out");
        return;
    };
    stage.report.check(
        "Skeleton",
        "the block is animating",
        stage.animations(node) > 0,
        &format!(
            "the engine reports {} running on it",
            stage.animations(node)
        ),
    );
    find::mark(stage, "skeleton:block", rect);

    for sample in 0..SAMPLES {
        shot_of(
            stage,
            &format!("vd-pulse-{sample:02}"),
            verdict::grown(rect, MARGIN),
        );
        stage.wait(APART);
    }
}
