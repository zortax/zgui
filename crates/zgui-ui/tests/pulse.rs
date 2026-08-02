//! What a skeleton's pulse puts on the screen, output frame by output frame.
//!
//! # Why this is measured in composed pixels and nothing else
//!
//! Every other way of asking is satisfied by the defect. The values the animation computes are
//! smooth; the display list carries them frame for frame; the damage covers exactly the box that
//! changed. The picture still flickers, because what a person looks at is the *composite*, and a
//! translucent box's composite is not a monotone function of its opacity: the box's colour is
//! multiplied by the alpha and quantised to the eight bits the surface holds, and what is behind it
//! is weighted by that same alpha quantised separately. One number rounded twice, so over a fade the
//! whole block steps up, back down and up again by one level — all of it at once, which is exactly
//! what "flickers slightly" looks like.
//!
//! So the fixture reads one pixel of the block per output frame and asks the question somebody
//! looking at it asks: does it get steadily darker and then steadily lighter, or does it jitter on
//! the way?
//!
//! # Why the amplitude is asserted beside the shape
//!
//! A block that never moves at all is perfectly monotone. The swing is asserted too, so a pulse
//! that stopped, or one so faint that the noise on it is a fifth of it, fails here rather than
//! passing as smooth.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point};
use zgui::view;
use zgui::view::AnyView;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// The page the fixture is laid out on: a card, because that is what a skeleton sits on and what
/// decides how much of a fade towards it there is to see.
///
/// The card is deliberately dark. The pulse is an opacity fade — the accent block towards
/// whatever is behind it, which is how the reference skeleton breathes — so on a card the block's
/// own colour the swing is a few levels of grey however well it runs. Against a dark card the
/// same fade is scores of levels, so the amplitude assertion measures the pulse rather than the
/// palette's distance from its surface.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .card { padding: 24px; gap: 12px; align-items: stretch; width: 400px;
                             background-color: #303030 }
                     .bar { height: 16px }";

/// How many output frames one turn of the pulse takes: 2000ms at sixty hertz.
const PERIOD: usize = 120;

/// The least the block may move, in levels of grey, over one turn.
///
/// The pulse it replaced moved about five on a card, which is the same order as the noise that was
/// riding on it — so this is well above what a fade towards the surface behind can produce, and a
/// return to one would fail here.
const SWING: u8 = 10;

/// Where the reading is taken: inside the first bar, away from its rounded corners.
const AT: Point<DevicePx, Device> = Point::new(DevicePx(200.0), DevicePx(32.0));

/// Three skeletons on a card, sized the way a caller sizes them.
fn page() -> AnyView {
    AnyView::new(view! {
        ThemeProvider {
            column(class = "card") {
                Skeleton(class = "bar")
                Skeleton(class = "bar")
                Skeleton(class = "bar")
            }
        }
    })
}

/// The grey level at [`AT`] on each of `frames` consecutive output frames.
fn levels(stage: &mut Stage, frames: usize) -> Vec<u8> {
    let mut series = Vec::with_capacity(frames);
    for _ in 0..frames {
        stage.tick();
        series.push(stage.colour_at(AT).0);
    }
    series
}

/// Asserts that `series` falls and then rises, with no step the other way anywhere.
///
/// The turning point is found rather than assumed, because where in its cycle the animation is when
/// the fixture starts reading is not something the fixture decides.
fn assert_one_turn(series: &[u8], what: &str) {
    let low = series
        .iter()
        .enumerate()
        .min_by_key(|(_, level)| **level)
        .map(|(index, _)| index)
        .expect("the fixture read at least one frame");
    for pair in series[..=low].windows(2) {
        assert!(
            pair[1] <= pair[0],
            "{what}: the block lightened on its way down — {series:?}"
        );
    }
    for pair in series[low..].windows(2) {
        assert!(
            pair[1] >= pair[0],
            "{what}: the block darkened on its way back up — {series:?}"
        );
    }
}

#[test]
fn a_skeleton_pulses_without_stepping_backwards() {
    let Some(mut stage) = Stage::open(SHEET, page) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let series = levels(&mut stage, PERIOD);
    let high = series.iter().copied().max().expect("frames were read");
    let low = series.iter().copied().min().expect("frames were read");
    assert!(
        high - low >= SWING,
        "the pulse moved {} levels of grey over a whole turn, which is not a pulse: {series:?}",
        high - low
    );
    assert_one_turn(&series, "the first turn");
}

/// The pulse repeats, and the second turn is the same shape as the first.
///
/// Not a restatement of the first case: an animation that runs once and stops, or one that restarts
/// from its beginning part-way through, produces a perfectly smooth first turn.
#[test]
fn a_skeleton_goes_on_pulsing_the_same_way() {
    let Some(mut stage) = Stage::open(SHEET, page) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let _first = levels(&mut stage, PERIOD);
    let second = levels(&mut stage, PERIOD);
    let high = second.iter().copied().max().expect("frames were read");
    let low = second.iter().copied().min().expect("frames were read");
    assert!(
        high - low >= SWING,
        "the pulse had stopped by its second turn: {second:?}"
    );
    assert_one_turn(&second, "the second turn");
}

/// How many turns the long reading covers, and how far into the first one it starts.
///
/// Both matter. The two cases above read exactly one period each, starting from the frame the
/// animation itself started on — so the moment the pulse turns over and begins again falls on the
/// boundary between two readings and is never inside one. That is the one frame in the whole cycle
/// where the value is computed from a fresh iteration rather than from the one before it, and a
/// fixture whose window never contains it cannot say anything about it at all.
const TURNS: usize = 4;

/// A quarter of a period, so the wrap lands in the middle of what is read rather than at its edge.
const OFFSET: usize = PERIOD / 4;

/// The most a single output frame may move the block, in levels of grey.
///
/// The whole swing — around a hundred levels on this card — is spread over half a period, and the
/// easing concentrates its slope mid-fade, so an even pulse peaks at about four levels a frame.
/// A step twice that is a frame that sampled the animation somewhere other than where its own
/// place in the sequence put it — which is what a person sees as a flicker rather than as a fade.
const STEP: i32 = 6;

#[test]
fn a_pulse_crossing_its_own_wrap_moves_by_the_same_small_steps_as_everywhere_else() {
    let Some(mut stage) = Stage::open(SHEET, page) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let _offset = levels(&mut stage, OFFSET);
    let series = levels(&mut stage, PERIOD * TURNS);
    let steps: Vec<i32> = series
        .windows(2)
        .map(|pair| i32::from(pair[1]) - i32::from(pair[0]))
        .collect();
    let jump = steps.iter().copied().map(i32::abs).max().unwrap_or(0);
    assert!(
        jump <= STEP,
        "one frame moved the block {jump} levels of grey: {series:?}"
    );
    // And it is still a pulse over the whole reading, so a block that stopped moving — which has no
    // steps at all and therefore no large ones — fails here rather than passing as smooth.
    let high = series.iter().copied().max().expect("frames were read");
    let low = series.iter().copied().min().expect("frames were read");
    assert!(
        high - low >= SWING,
        "the pulse moved {} levels of grey over {TURNS} turns: {series:?}",
        high - low
    );
    // Every reversal in the reading is a turning point of the animation, and a pulse has exactly
    // two per period: nothing else may double back.
    let reversals = steps
        .windows(2)
        .filter(|pair| pair[0] * pair[1] < 0)
        .count();
    assert!(
        reversals <= 2 * TURNS + 1,
        "the block changed direction {reversals} times over {TURNS} turns: {series:?}"
    );
}
