//! The handle an application holds, and the signals the panel reads.
//!
//! One object with two faces. The [`probe`](DevTools::probe) side is written from the end of every
//! frame with what that frame did; the view side reads the same signals and draws them. Keeping
//! them in one place is what makes the panel *live* — nothing is passed between the two, so there
//! is no moment at which the panel is showing a frame that has already been replaced.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view::NodeId;

use crate::sample::{Element, Frame, Stage};

/// Which panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tab {
    /// The picked element: what it is, what box it got and what the cascade computed for it.
    #[default]
    Element,
    /// What the last frame did: batches, passes, damage and every counter that moved.
    Frame,
    /// Where the last frame's time went, stage by stage.
    Timeline,
    /// What this build of the style engine supports, and what it does not.
    Parity,
    /// What the renderer is holding on the device.
    Memory,
}

impl Tab {
    /// Every panel, in the order the tab strip lists them.
    pub const ALL: [Self; 5] = [
        Self::Element,
        Self::Frame,
        Self::Timeline,
        Self::Parity,
        Self::Memory,
    ];

    /// What the tab is called.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Element => "Element",
            Self::Frame => "Frame",
            Self::Timeline => "Timeline",
            Self::Parity => "Parity",
            Self::Memory => "Memory",
        }
    }
}

/// Everything the inspector shows, and the handle that fills it in.
///
/// Every field is a signal, and a signal is a handle rather than a value, so this is `Copy`: the
/// probe holds one, the panel holds one, every tab holds one, and all of them are the same
/// inspector. That is worth the eight fields — a handle behind a reference count would have to be
/// cloned into each of those places, and a view body that clones is a view body whose closures
/// cannot be `Fn`.
#[derive(Clone, Copy)]
pub struct DevTools {
    /// Whether the panel is showing.
    pub(crate) open: RwSignal<bool, LocalStorage>,
    /// Whether the next pointer move picks what is under it.
    pub(crate) picking: RwSignal<bool, LocalStorage>,
    /// Whether the probe has stopped writing, so the panel holds one frame still.
    pub(crate) frozen: RwSignal<bool, LocalStorage>,
    /// Which panel is showing.
    pub(crate) tab: RwSignal<Tab, LocalStorage>,
    /// What is picked.
    pub(crate) picked: RwSignal<Option<NodeId>, LocalStorage>,
    /// What the cascade and the layout say about it.
    pub(crate) element: RwSignal<Option<Element>, LocalStorage>,
    /// What the last frame did.
    pub(crate) frame: RwSignal<Frame, LocalStorage>,
    /// Where the last frame's time went.
    pub(crate) timeline: RwSignal<Vec<Stage>, LocalStorage>,
}

impl Default for DevTools {
    fn default() -> Self {
        Self::new()
    }
}

impl DevTools {
    /// A closed inspector with nothing picked.
    #[must_use]
    pub fn new() -> Self {
        // The frame timeline is read out of the marks the frame loop already writes, and those are
        // kept only for somebody who asked. Asking here rather than when the panel opens means the
        // first frame after it opens has a timeline in it: a ring turned on at that moment would
        // show one empty strip and then fill, which reads as "the frame did nothing".
        zgui_profile::latency::retain(RING);
        Self {
            open: RwSignal::new_local(false),
            picking: RwSignal::new_local(false),
            frozen: RwSignal::new_local(false),
            tab: RwSignal::new_local(Tab::default()),
            picked: RwSignal::new_local(None),
            element: RwSignal::new_local(None),
            frame: RwSignal::new_local(Frame::default()),
            timeline: RwSignal::new_local(Vec::new()),
        }
    }

    /// The probe to hand to [`with_probe`](zgui::App::with_probe).
    ///
    /// Without it the panel opens, the tab strip works and every value in it is empty: a view can
    /// see the document but not the frame that painted it.
    #[must_use]
    pub fn probe(&self) -> Rc<dyn zgui::runtime::FrameProbe> {
        Rc::new(crate::probe::Sampler::new(*self))
    }

    /// Whether the panel is showing.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.get_untracked()
    }

    /// Opens or closes the panel.
    pub fn set_open(&self, open: bool) {
        self.open.set(open);
    }

    /// Starts or stops picking: while it is on, what the pointer is over is what is shown.
    pub fn set_picking(&self, picking: bool) {
        self.picking.set(picking);
    }

    /// Freezes the panel on the frame it is showing, or lets it follow again.
    ///
    /// A frozen panel is also a quiet one: the probe returns at once, so the window stops being
    /// woken by the inspector and idles with the panel still on screen. That is what makes a value
    /// that only exists for one frame readable.
    pub fn set_frozen(&self, frozen: bool) {
        self.frozen.set(frozen);
    }

    /// Which panel is showing.
    #[must_use]
    pub fn tab(&self) -> Tab {
        self.tab.get_untracked()
    }

    /// Shows `tab`.
    pub fn show(&self, tab: Tab) {
        self.tab.set(tab);
    }

    /// Picks `node`, opening the panel if it was closed.
    pub fn pick(&self, node: NodeId) {
        self.picked.set(Some(node));
        self.open.set(true);
    }
}

/// How many latency marks are kept for the timeline.
///
/// A frame writes on the order of twenty, so this is a few hundred frames — far more than the strip
/// shows, and deliberately: the marks of the frame being *drawn* are still being written while the
/// strip is built, so what it renders is the frame before, and a ring that held exactly one frame
/// would always be showing a half-written one.
const RING: usize = 4096;
