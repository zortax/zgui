//! How many vector passes a frame plans, and how much of the surface they rasterise.
//!
//! A pass is a rasterisation of a region of the surface, so the area of a frame's passes sets what
//! its scratch space costs. This reports both, over a scroll, from the compositor's plan of each
//! frame.
//!
//! The sample is taken **as the frame is drawn** and not afterwards. A frame's pass plan is a
//! function of the damage it answered, and by the time the loop has gone quiet the damage has been
//! retired and the last plan is the empty one belonging to a frame that redrew nothing.

use iceblink::{
    BufferAge, Frame as Built, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer,
};

use crate::input::wheel;
use crate::phase::Driver;
use crate::verify::repaint_everything;

/// How many wheel notches the measurement drives.
const NOTCHES: usize = 600;

/// How often the scroll changes direction, in notches.
const REVERSE: usize = 60;

/// What one frame's pass plan came to.
#[derive(Clone, Copy, Default)]
struct Frame {
    /// How many passes it planned.
    passes: usize,
    /// How many device pixels those passes rasterise between them.
    area: i64,
    /// Whether the frame redrew the whole surface.
    full: bool,
}

/// Every frame drawn while the measurement is running.
///
/// A static because the renderer a window draws through is built before any phase exists and
/// knows nothing about which one is running; `None` is what makes every other phase pay nothing at
/// all for this one.
static FRAMES: std::sync::Mutex<Option<Vec<Frame>>> = std::sync::Mutex::new(None);

/// Runs `f` on the measured frames.
fn with_frames<T>(f: impl FnOnce(&mut Option<Vec<Frame>>) -> T) -> T {
    f(&mut FRAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner))
}

/// Records what one frame's vector work costs, when the measurement is running.
fn observe(frame: &Built) {
    with_frames(|held| {
        let Some(frames) = held.as_mut() else {
            return;
        };
        let plan = &frame.vector_plan;
        if plan.is_empty() {
            return;
        }
        frames.push(Frame {
            passes: plan.passes.len(),
            area: plan
                .passes
                .iter()
                .map(|pass| i64::from(pass.region.size.width) * i64::from(pass.region.size.height))
                .sum(),
            full: frame.damage.draw.is_full(),
        });
    });
}

/// A renderer whose frames are sampled for the pass measurement as they are drawn.
pub(crate) struct Planned<R> {
    inner: R,
}

impl<R: Renderer> Planned<R> {
    /// Returns `inner` with its frames sampled.
    pub(crate) fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R: Renderer> Renderer for Planned<R> {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn target_age(&self) -> BufferAge {
        crate::draw::age(self.inner.target_age())
    }

    fn draw(&mut self, frame: &Built) -> FrameOutcome {
        observe(frame);
        crate::draw::drew();
        self.inner.draw(frame)
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }
}

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    if phase != "passes" {
        return None;
    }
    let middle = driver.middle;
    let mut drawn = 0;
    with_frames(|held| *held = Some(Vec::new()));
    // A whole repaint first and last, because that is where the pass count is largest: a scrolled
    // frame is damage-culled to the strip that moved, and the frame that plans hundreds of passes is
    // the one that redraws the document.
    repaint_everything(&mut driver.harness);
    for notch in 0..NOTCHES {
        let lines = if (notch / REVERSE).is_multiple_of(2) {
            -3.0
        } else {
            3.0
        };
        driver.harness.deliver_to_first(wheel(middle, lines));
        drawn += driver.harness.settle(64);
    }
    repaint_everything(&mut driver.harness);
    let frames = with_frames(Option::take).unwrap_or_default();
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
    let mut passes: Vec<usize> = frames.iter().map(|frame| frame.passes).collect();
    let mut areas: Vec<i64> = frames.iter().map(|frame| frame.area).collect();
    passes.sort_unstable();
    areas.sort_unstable();
    let worst = frames
        .iter()
        .max_by_key(|frame| frame.passes)
        .copied()
        .unwrap_or_default();
    println!(
        "{label} frames={} passes[p50={} max={}] area_px[p50={} max={}] \
         worst_frame[passes={} area_px={}]",
        frames.len(),
        passes[passes.len() / 2],
        passes[passes.len() - 1],
        areas[areas.len() / 2],
        areas[areas.len() - 1],
        worst.passes,
        worst.area,
    );
}
