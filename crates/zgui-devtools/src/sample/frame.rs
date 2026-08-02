//! What one frame did: what it drew, what it damaged, what it decided to redo, and what the
//! renderer is holding because of it.
//!
//! Nothing here is a running total, and in particular there is no frame *number*. A value that
//! moves every frame is a value the panel would have to be redrawn for every frame, and an
//! inspector that redraws unconditionally is one that stops the window it is inspecting from ever
//! idling — so the only things published are the ones that describe what the frame did.
//!
//! The counters are read as a *delta*, not as a total. A total tells you how long the window has
//! been open; the delta tells you what this frame cost, which is the only form in which "hovering
//! one row restyled four hundred elements" is a sentence anybody can act on.

use zgui::geom::{Device, Rect};
use zgui::render::MemoryReport;
use zgui::runtime::Window;
use zgui::runtime::budget::{CacheId, CacheUnit};

/// What the last frame did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Frame {
    /// How many primitives are in the display list.
    pub(crate) primitives: usize,
    /// How many batches the scene was sorted into.
    pub(crate) batches: usize,
    /// How many rasterisation passes were planned for its vector work.
    pub(crate) passes: usize,
    /// The rectangles this frame drew against, in device pixels.
    pub(crate) damage: Vec<Rect<i32, Device>>,
    /// Whether the frame drew against the whole surface.
    pub(crate) full_damage: bool,
    /// How big the surface is, in device pixels, which is what the damage is a fraction of.
    pub(crate) surface: (i32, i32),
    /// Every counter that moved during the frame, largest first.
    pub(crate) counters: Vec<(&'static str, u64)>,
    /// What the renderer holds on the device.
    pub(crate) memory: MemoryReport,
    /// What every budgeted cache is holding, and the level each states.
    pub(crate) budget: Vec<Held>,
}

/// One cache's occupancy, as the panel shows it.
///
/// Deliberately *not* the whole [`CacheReport`](zgui::runtime::budget::CacheReport). That carries
/// the frame each cache was last read in, which moves every frame for every cache the document is
/// drawing from — and a sample that differed every frame would be published every frame, which is
/// a window that never settles while the panel is open. What is kept is what a person reading a
/// memory panel is looking at: how much is held, how much of it is pinned, and the level it is
/// held to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    /// Which cache.
    pub(crate) id: CacheId,
    /// What it is holding.
    pub(crate) resident: u64,
    /// How much of that is held by something still using it.
    pub(crate) pinned: u64,
    /// The level it states, if it states one.
    pub(crate) limit: Option<u64>,
    /// What those figures are counted in.
    pub(crate) unit: CacheUnit,
}

impl Held {
    /// How far over its level the cache is, or zero when it is under one or states none.
    pub(crate) fn over(&self) -> u64 {
        match self.limit {
            None => 0,
            Some(limit) => self.resident.saturating_sub(limit),
        }
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            primitives: 0,
            batches: 0,
            passes: 0,
            damage: Vec::new(),
            full_damage: false,
            surface: (0, 0),
            counters: Vec::new(),
            memory: MemoryReport::ZERO,
            budget: Vec::new(),
        }
    }
}

impl Frame {
    /// How much of the surface this frame redrew, as a fraction of it.
    pub(crate) fn damage_fraction(&self) -> f64 {
        if self.full_damage {
            return 1.0;
        }
        let whole = f64::from(self.surface.0) * f64::from(self.surface.1);
        if whole <= 0.0 {
            return 0.0;
        }
        let drawn: f64 = self
            .damage
            .iter()
            .map(|rect| f64::from(rect.size.width) * f64::from(rect.size.height))
            .sum();
        (drawn / whole).min(1.0)
    }
}

/// Reads the frame that has just finished on `window`.
///
/// `moved` is what the counters did since this was last read, which is not necessarily since the
/// previous frame: what a frame did is published on a cadence, and the frames in between are
/// summed into the next publication rather than dropped.
pub(crate) fn sample_frame(
    window: &Window,
    moved: &std::collections::BTreeMap<&'static str, u64>,
) -> Frame {
    let scene = window.scene();
    let damage = window.damage();
    let mut counters: Vec<(&'static str, u64)> = moved
        .iter()
        .filter(|(_, value)| **value > 0)
        .map(|(counter, value)| (*counter, *value))
        .collect();
    counters.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    Frame {
        primitives: scene.primitives.len(),
        batches: scene.batches().count(),
        passes: scene.pass_plan().passes.len(),
        damage: damage.rects().to_vec(),
        full_damage: damage.is_full(),
        surface: (scene.viewport().width, scene.viewport().height),
        counters,
        memory: window.renderer().memory(),
        budget: window
            .last_budget_report()
            .lines()
            .map(|line| Held {
                id: line.id,
                resident: line.report.resident,
                pinned: line.report.pinned,
                limit: line.limit,
                unit: line.report.unit,
            })
            .collect(),
    }
}
