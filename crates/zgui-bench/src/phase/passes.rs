//! How many vector passes a frame plans, and how many scratch layers they need between them.
//!
//! Two questions, and the second is the one a budget answers. A pass is a rasterisation of a region
//! of the surface; passes that do not meet on the surface can be rasterised into one layer, so what
//! a scratch texture costs is not how many passes a frame plans but how deeply they stack over each
//! other. This reports both, over a scroll, from the document's own pass plan rather than from a
//! device.
//!
//! The sample is taken **as the frame is drawn** and not afterwards. A frame's pass plan is a
//! function of the damage it answered, and by the time the loop has gone quiet the damage has been
//! retired and the last plan is the empty one belonging to a frame that redrew nothing.

use std::cell::RefCell;

use zgui::bits::DamageSet;
use zgui::render::Layering;
use zgui::scene::Scene;

use crate::input::wheel;
use crate::phase::Driver;
use crate::verify::repaint_everything;

/// The most layers a rasteriser has.
const CEILING: u32 = 64;

/// How many wheel notches the measurement drives.
const NOTCHES: usize = 600;

/// How often the scroll changes direction, in notches.
const REVERSE: usize = 60;

/// What one frame's pass plan came to.
#[derive(Clone, Copy, Default)]
struct Frame {
    /// How many passes it planned.
    passes: usize,
    /// How many scratch layers those passes need between them.
    depth: u32,
    /// Whether any of them could not be given one.
    over: bool,
    /// Whether the frame redrew the whole surface.
    full: bool,
}

thread_local! {
    /// Every frame drawn while the measurement is running.
    ///
    /// A thread local because the renderer a window draws through is built before any phase exists
    /// and knows nothing about which one is running; `None` is what makes every other phase pay
    /// nothing at all for this one.
    static FRAMES: RefCell<Option<Vec<Frame>>> = const { RefCell::new(None) };
}

/// Records what one frame's vector work would cost, when the measurement is running.
pub(crate) fn observe(scene: &Scene, damage: &DamageSet) {
    FRAMES.with_borrow_mut(|held| {
        let Some(frames) = held.as_mut() else {
            return;
        };
        let plan = scene.pass_plan();
        if plan.is_empty() {
            return;
        }
        let regions: Vec<_> = plan.passes.iter().map(|pass| pass.region).collect();
        let layering = Layering::of(&regions, CEILING);
        frames.push(Frame {
            passes: regions.len(),
            depth: layering.layers(),
            over: layering.placed() < regions.len(),
            full: damage.is_full(),
        });
    });
}

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    if phase != "passes" {
        return None;
    }
    let middle = driver.middle;
    let mut drawn = 0;
    FRAMES.with_borrow_mut(|held| *held = Some(Vec::new()));
    // A whole repaint first and last, because that is where the pass count is largest: a scrolled
    // frame is damage-culled to the strip that moved, and the frame that plans hundreds of passes is
    // the one that redraws the document.
    repaint_everything(&mut driver.harness, false);
    for notch in 0..NOTCHES {
        let lines = if (notch / REVERSE).is_multiple_of(2) {
            -3.0
        } else {
            3.0
        };
        driver.harness.deliver_to_first(wheel(middle, lines));
        drawn += driver.harness.settle(64);
    }
    repaint_everything(&mut driver.harness, false);
    let frames = FRAMES.with_borrow_mut(Option::take).unwrap_or_default();
    report("REPAINT", &frames, true);
    report("SCROLL", &frames, false);
    Some(drawn)
}

/// Prints what the whole-surface frames, or the damage-culled ones, came to.
fn report(label: &str, all: &[Frame], full: bool) {
    let frames: Vec<Frame> = all
        .iter()
        .copied()
        .filter(|frame| frame.full == full)
        .collect();
    if frames.is_empty() {
        println!("{label} frames=0 (no such frame planned any vector work)");
        return;
    }
    let frames = &frames[..];
    let mut passes: Vec<usize> = frames.iter().map(|frame| frame.passes).collect();
    let mut depths: Vec<u32> = frames.iter().map(|frame| frame.depth).collect();
    passes.sort_unstable();
    depths.sort_unstable();
    let worst = frames
        .iter()
        .max_by_key(|frame| frame.passes)
        .copied()
        .unwrap_or_default();
    println!(
        "{label} frames={} passes[p50={} max={}] depth[p50={} max={}] \
         worst_frame[passes={} depth={}] over_four_layers={} over_the_ceiling={}",
        frames.len(),
        passes[passes.len() / 2],
        passes[passes.len() - 1],
        depths[depths.len() / 2],
        depths[depths.len() - 1],
        worst.passes,
        worst.depth,
        frames.iter().filter(|frame| frame.depth > 4).count(),
        frames.iter().filter(|frame| frame.over).count(),
    );
    let total: usize = frames.iter().map(|frame| frame.passes).sum();
    let layers: u64 = frames.iter().map(|frame| u64::from(frame.depth)).sum();
    println!(
        "{label} PACKING passes_per_layer={:.1} rasterisations_saved={}",
        total as f64 / layers.max(1) as f64,
        total as u64 - layers,
    );
}
