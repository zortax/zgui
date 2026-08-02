//! The phases: one interaction, or one comparison, driven over one document size.
//!
//! A *phase* is what somebody reaches for once a band has gone red — an exploratory measurement or
//! a differential, run at a size they choose. It is not the ratchet, which is
//! [`scenario`](crate::scenario) and decides its own document.
//!
//! There are three groups, and what separates them is what they ask of the document rather than how
//! they are driven: [`interaction`] changes what is in the window, [`geometry`] changes the window,
//! and [`differential`] changes nothing and compares two pictures of the result.

/// Everything one interaction moved, measured around a body that drives it.
macro_rules! interaction {
    ($ledger:expr, $body:block) => {{
        let before = zgui_profile::counter::snapshot();
        let started = std::time::Instant::now();
        let frames: u64 = $body;
        let elapsed = started.elapsed();
        let after = zgui_profile::counter::snapshot();
        $ledger.push(elapsed, frames, before.delta(&after));
        frames
    }};
}

mod differential;
mod geometry;
mod interaction;
mod passes;

use std::rc::Rc;

use zgui::geom::{Css, CssPx, Point};
use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::gallery::Scheme;
use crate::stats::Ledger;
use crate::verify::FullFrame;

/// Everything a phase is driven with: one open window, and everything read off it once.
///
/// One struct rather than a dozen arguments because every phase needs a different half of it and no
/// two need the same half. It is built once, in `main`, immediately after the window is opened —
/// which is the only moment at which the swatch centres, the document size and the recorder are all
/// known and none of them has been disturbed by anything a phase did.
pub(crate) struct Driver {
    /// The driven application.
    pub(crate) harness: Harness<Runtime>,
    /// The colour-scheme signal the mounted gallery reads, which is what a script flips.
    pub(crate) scheme: Scheme,
    /// The recorder holding what a full repaint of this window drew.
    pub(crate) full: Rc<FullFrame>,
    /// The centre of each of the four probe swatches, in CSS pixels.
    pub(crate) centres: Vec<Point<CssPx, Css>>,
    /// The middle of the window, which is what a wheel is aimed at.
    pub(crate) middle: Point<CssPx, Css>,
    /// A corner nothing interactive is under, which is where a pointer goes to stop hovering.
    pub(crate) away: Point<CssPx, Css>,
    /// How many boxes the document laid out.
    pub(crate) boxes: usize,
    /// How many fragments those boxes produced.
    pub(crate) fragments: usize,
    /// Which document size this is.
    pub(crate) size: String,
    /// How many times the phase repeats its interaction.
    pub(crate) repeats: usize,
    /// What the user feels: one input event and every frame before the loop went quiet.
    pub(crate) ledger: Ledger,
    /// The second half of an interaction that has one — the glide after the notch, the release
    /// after the press.
    pub(crate) ticks: Ledger,
}

pub(crate) use crate::phase::passes::observe;

/// Runs the phase called `phase`, and returns how many frames it drew.
///
/// # Panics
///
/// Panics when no group claims the name, naming it. A phase that is silently not run is a run whose
/// last line says it succeeded.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> u64 {
    interaction::run(driver, phase)
        .or_else(|| geometry::run(driver, phase))
        .or_else(|| passes::run(driver, phase))
        .or_else(|| differential::run(driver, phase))
        .unwrap_or_else(|| panic!("unknown phase {phase}"))
}
