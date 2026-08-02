//! The end of every frame, turned into what the panel shows.
//!
//! Three rules keep the inspector from becoming the thing it is measuring.
//!
//! **Nothing is sampled while the panel is closed.** A closed inspector writes no signal, so it
//! asks for no frame, so a window with the crate linked in and the panel shut idles exactly as one
//! without it does.
//!
//! **Nothing is written that has not changed.** Every signal is compared before it is set, and
//! nothing published is a running total or a frame number. A frame that repeated the last one
//! therefore leaves the panel untouched and asks for nothing further, so a still document idles
//! with the panel open.
//!
//! **What cannot help but change is published on a cadence rather than every frame.** Two of the
//! samples move on a document nobody is touching: a counter delta and a stage duration both
//! include the panel's own re-render, so comparing them before writing them cannot converge — the
//! act of showing the number changes the number. Published every *n*th frame instead, the frames
//! between two publications write nothing, and a window with nothing else to do idles there. That
//! is what makes the fixed point reachable at all: an idle window runs no frames, so the probe
//! stops running too, and the cadence never comes back round on its own.
//!
//! **The computed style is read only when the picked element changes.** Serialising a few hundred
//! longhands is the most expensive thing here by an order of magnitude, and the answer only moves
//! when the cascade does.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use zgui::prelude::*;
use zgui::runtime::{FrameProbe, Window};
use zgui::view::NodeId;

use crate::sample::{sample_element, sample_frame, sample_timeline};
use crate::state::DevTools;

/// How many frames pass between publications of the two samples that move every frame.
///
/// About half a second at 60 Hz, and the number is not really the point — *that there is one* is.
/// A counter delta and a stage duration differ every single frame, including on a document nobody
/// is touching, because the panel's own re-render is itself work the frame did. Published every
/// frame they are a closed loop: the sample differs, so it is written, so the panel re-renders, so
/// the next frame's sample differs. Published on a cadence the loop has somewhere to stop — the
/// frames between two publications write nothing, the window finds nothing left to do, and it
/// idles. A window that has idled runs no frames, so the probe does not run either and the
/// cadence never comes round again until something real happens.
const CADENCE: u32 = 30;

/// What is installed with [`DevTools::probe`](crate::DevTools::probe).
pub(crate) struct Sampler {
    /// Where what it reads is published.
    tools: DevTools,
    /// Every counter as of the end of the previous frame, so this frame's delta can be taken.
    previous: Cell<zgui_profile::Counters>,
    /// What was picked when the style listing was last built.
    styled: Cell<Option<NodeId>>,
    /// How many elements the previous frame restyled, so a picked element whose style may have
    /// moved is read again.
    restyles: Cell<u64>,
    /// How many frames until the next publication of what moves every frame.
    countdown: Cell<u32>,
    /// Which tab was showing when this last published, so a tab that has just been opened does not
    /// wait out a cadence before it says anything.
    showing: Cell<crate::Tab>,
    /// The counter delta accumulated since the last publication.
    ///
    /// Summed rather than sampled: the point of the cadence is that the frames in between are not
    /// published, not that what they did is thrown away.
    pending: RefCell<BTreeMap<&'static str, u64>>,
}

impl Sampler {
    /// A sampler publishing into `tools`.
    pub(crate) fn new(tools: DevTools) -> Self {
        Self {
            tools,
            previous: Cell::new(zgui_profile::counter::snapshot()),
            styled: Cell::new(None),
            restyles: Cell::new(0),
            countdown: Cell::new(0),
            showing: Cell::new(crate::Tab::default()),
            pending: RefCell::new(BTreeMap::new()),
        }
    }

    /// Whether this frame is one of the ones that publishes.
    ///
    /// A tab that has just been shown publishes at once, whatever the countdown says: a panel that
    /// answered "nothing yet" for half a second after being asked would read as broken.
    fn due(&self, tab: crate::Tab) -> bool {
        if self.showing.replace(tab) != tab {
            self.countdown.set(0);
        }
        match self.countdown.get() {
            0 => {
                self.countdown.set(CADENCE);
                true
            }
            left => {
                self.countdown.set(left - 1);
                false
            }
        }
    }
}

impl FrameProbe for Sampler {
    fn frame_ended(&self, window: &Window) {
        let state = &self.tools;
        if !state.open.get_untracked() || state.frozen.get_untracked() {
            return;
        }
        let now = zgui_profile::counter::snapshot();
        let moved = self.previous.replace(now).delta(&now);
        let tab = state.tab.get_untracked();
        let due = self.due(tab);

        {
            let mut pending = self.pending.borrow_mut();
            for (counter, value) in moved.iter().filter(|(_, value)| *value > 0) {
                *pending.entry(counter.name()).or_default() += value;
            }
        }
        if due {
            let frame = sample_frame(window, &self.pending.borrow());
            self.pending.borrow_mut().clear();
            if state.frame.get_untracked() != frame {
                state.frame.set(frame);
            }
        }

        // Only while its own tab is showing, and only on the cadence. Stage durations move by a
        // nanosecond or two every frame whatever the window is doing, so publishing them
        // unconditionally would mean the panel changed every frame and the window never settled —
        // with the timeline hidden, which is most of the time, that would be a window kept awake
        // for a number nobody is looking at.
        if due && tab == crate::Tab::Timeline {
            let timeline = sample_timeline();
            if state.timeline.get_untracked() != timeline {
                state.timeline.set(timeline);
            }
        }

        let picked = state.picked.get_untracked();
        // The style listing is rebuilt when the picked element changes, and also when the frame
        // restyled anything: a picked element whose rules now match differently would otherwise
        // keep showing the values it had when it was picked, which is the one failure that makes an
        // inspector actively misleading rather than merely stale.
        let restyle = self.styled.get() != picked || moved.elements_restyled != self.restyles.get();
        self.restyles.set(moved.elements_restyled);
        let element = picked.and_then(|node| sample_element(window, node, restyle));
        if restyle {
            self.styled.set(picked);
        }
        match (element, state.element.get_untracked()) {
            // A frame that did not rebuild the listing has none to publish, so the one already
            // shown is carried forward rather than replaced with an empty one.
            (Some(fresh), Some(held)) if fresh.style.is_empty() && fresh.node == held.node => {
                let carried = crate::sample::Element {
                    style: held.style.clone(),
                    ..fresh
                };
                if carried != held {
                    state.element.set(Some(carried));
                }
            }
            (fresh, held) if fresh != held => state.element.set(fresh),
            _ => {}
        }
    }

    fn describe(&self) -> &str {
        "the zgui-devtools inspector"
    }
}
