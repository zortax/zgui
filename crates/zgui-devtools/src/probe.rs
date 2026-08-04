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

use zgui::geom::{Css, CssPx, Point, Rect, Size};
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

/// How many frames the frame-time graph keeps.
///
/// Half a minute at 60 Hz. Long enough to still hold a stutter somebody is turning round to
/// describe, and it costs one `f64` a frame — the graph reduces them to a couple of hundred columns
/// when it draws, so the length of this is a question about memory rather than about the drawing.
const HISTORY: usize = 1800;

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
    /// What the document's revision was when the tree was last read.
    ///
    /// The tree is the most expensive sample here — it is the whole document — and it only changes
    /// when the document does, which the revision answers in one comparison.
    revision: Cell<u64>,
    /// Which tree was last read, so switching between them shows the other one at once.
    mode: Cell<crate::state::TreeMode>,
    /// What was picked when the enclosing component was last resolved.
    aimed: Cell<Option<NodeId>>,
    /// How long each of the last few frames took, before it is published.
    history: RefCell<Vec<f64>>,
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
            revision: Cell::new(0),
            mode: Cell::new(crate::state::TreeMode::default()),
            aimed: Cell::new(None),
            history: RefCell::new(Vec::new()),
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
        if tab == crate::Tab::Timeline {
            // Every frame, because the chart's whole point is the frame that was slow — and that is
            // never the frame the panel happens to be showing. Kept off the signal until the
            // cadence comes round, so what accumulates is a vector rather than a redraw.
            if let Some(us) = crate::sample::frame_total_us() {
                let mut history = self.history.borrow_mut();
                // A ring in a vector: the oldest goes when it is full. `remove(0)` on eighteen
                // hundred `f64` is a memmove of fourteen kilobytes once a frame, which is nothing
                // beside the frame that just ran.
                if history.len() >= HISTORY {
                    history.remove(0);
                }
                history.push(us);
            }
            if due {
                let timeline = sample_timeline();
                if state.timeline.get_untracked() != timeline {
                    state.timeline.set(timeline);
                }
                let history = self.history.borrow().clone();
                if state.history.get_untracked() != history {
                    state.history.set(history);
                }
            }
        }

        // The tree, only while its own tab is showing, and only when the document it describes has
        // actually moved. The revision counts every batch of changes a view made, so two equal
        // readings mean there is nothing to rebuild — which is what makes a tab that samples the
        // whole document cost nothing on a document nobody is touching.
        //
        // The panel's own subtree is left out of the walk. Without that the tree would contain the
        // rows that draw it, so drawing it would grow it, and it would never converge.
        if tab == crate::Tab::Elements {
            let revision = window.dom().revision();
            let mode = state.tree_mode.get_untracked();
            let moved = self.revision.replace(revision) != revision;
            let switched = self.mode.replace(mode) != mode;
            if moved || switched || due {
                let ours: Vec<NodeId> = [
                    state.panel.get_untracked(),
                    state.overlay.get_untracked(),
                ]
                .into_iter()
                .flatten()
                .collect();
                let tree =
                    crate::sample::sample_tree(window, mode, &ours, state.app.get_untracked());
                if state.tree.get_untracked().as_deref() != Some(tree.as_ref()) {
                    state.tree.set(Some(tree));
                }
            }
        }

        // What is alive, only while its own tab is showing. It moves when components mount or
        // unmount, which is a document change, so the revision gates it exactly as it gates the
        // tree — and the cadence catches anything that changed without touching the document.
        if tab == crate::Tab::Reactivity {
            let revision = window.dom().revision();
            let moved = self.revision.replace(revision) != revision;
            if moved || due {
                let reactive = crate::sample::sample_reactive();
                if state.reactive.get_untracked() != reactive {
                    state.reactive.set(reactive);
                }
            }
        }

        // Where the outline goes. Published on every frame the panel is open rather than only on
        // the tree tab, because picking runs from whichever tab is showing — and it is one lookup
        // when something is hovered and nothing at all when nothing is.
        let outline = state
            .highlighted
            .get_untracked()
            .and_then(|node| outline_of(window, node));
        if state.highlight_box.get_untracked() != outline {
            state.highlight_box.set(outline);
        }

        let picked = state.picked.get_untracked();
        // Which component built what is picked, so the tree can select something a reader can see
        // when the pointer picked a node the components view does not have a row for. Resolved only
        // when the pick moves: it is a walk over one sibling list per level of the document, which
        // is cheap once and pointless every frame.
        if self.aimed.replace(picked) != picked {
            let inside = picked.and_then(|node| crate::sample::tree::component_of(window, node));
            if state.picked_component.get_untracked() != inside {
                state.picked_component.set(inside);
            }
        }
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

/// Where the outline for `node` goes, in CSS pixels, or nothing when it occupies no space.
///
/// An element is its own box. A component is a *marker*, which has no box at all — what it occupies
/// is whatever its content does, so the answer is the union of the boxes between its open marker
/// and its close. That is a walk along one sibling list, and only while something is hovered.
fn outline_of(window: &Window, node: NodeId) -> Option<Rect<CssPx, Css>> {
    let scale = window.scale().get();
    let host = window.host();
    let union = match extent_of(window, node) {
        None => host.window_box(node)?,
        Some(nodes) => nodes
            .into_iter()
            .filter_map(|inside| host.window_box(inside))
            .filter(|found| found.size.width.0 > 0.0 && found.size.height.0 > 0.0)
            .reduce(|left, right| left.union(right))?,
    };
    Some(Rect::new(
        Point::new(
            CssPx(union.origin.x.0 / scale),
            CssPx(union.origin.y.0 / scale),
        ),
        Size::new(
            CssPx(union.size.width.0 / scale),
            CssPx(union.size.height.0 / scale),
        ),
    ))
}

/// The nodes between `open` and its matching close, when `open` is a component boundary.
///
/// `None` for anything that is not one, which is every element and every marker belonging to a
/// conditional or a list.
fn extent_of(window: &Window, open: NodeId) -> Option<Vec<NodeId>> {
    use zgui::view::instrument::{self, MarkerRole};

    let MarkerRole::Open(tag) = instrument::at(open)? else {
        return None;
    };
    let document = window.document().borrow();
    if !zgui_view_dom::id::is_live(&document, open) {
        return None;
    }
    let index = zgui_view_dom::id::resolve(&document, open);
    let mut inside = Vec::new();
    let mut next = document.store().core(index).next_sibling();
    while let Some(sibling) = next {
        let id = zgui_view_dom::id::to_view(document.store().key_of(sibling));
        if instrument::at(id) == Some(MarkerRole::Close(tag.instance)) {
            break;
        }
        inside.push(id);
        next = document.store().core(sibling).next_sibling();
    }
    Some(inside)
}
