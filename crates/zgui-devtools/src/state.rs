//! The handle an application holds, and the signals the panel reads.
//!
//! One object with two faces. The [`probe`](DevTools::probe) side is written from the end of every
//! frame with what that frame did; the view side reads the same signals and draws them. Keeping
//! them in one place is what makes the panel *live* — nothing is passed between the two, so there
//! is no moment at which the panel is showing a frame that has already been replaced.

use std::collections::HashSet;
use std::rc::Rc;

use zgui::geom::{Css, CssPx, Rect};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view::NodeId;

use crate::sample::{Element, Frame, Reactive, Stage, Tree};

/// Which panel is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tab {
    /// The document as a tree, and what the cascade and the layout say about what is picked.
    ///
    /// One tab rather than two, because they are one question asked twice: *which* element, and
    /// *what about it*. Picking in one and reading the answer in the other meant a tab switch
    /// between every step of the only workflow the panel has.
    #[default]
    Elements,
    /// What the last frame did: batches, passes, damage and every counter that moved.
    Frame,
    /// Where the last frame's time went, stage by stage.
    Timeline,
    /// What the reactive graph is holding, as far as it can be asked.
    Reactivity,
    /// What this build of the style engine supports, and what it does not.
    Parity,
    /// What the renderer is holding on the device.
    Memory,
}

impl Tab {
    /// Every panel, in the order the tab strip lists them.
    pub const ALL: [Self; 6] = [
        Self::Elements,
        Self::Frame,
        Self::Timeline,
        Self::Reactivity,
        Self::Parity,
        Self::Memory,
    ];

    /// The drawing in front of its name.
    pub(crate) const fn icon(self) -> &'static str {
        match self {
            Self::Elements => crate::panel::icon::ELEMENTS,
            Self::Frame => crate::panel::icon::FRAME,
            Self::Timeline => crate::panel::icon::TIMELINE,
            Self::Reactivity => crate::panel::icon::REACTIVITY,
            Self::Parity => crate::panel::icon::PARITY,
            Self::Memory => crate::panel::icon::MEMORY,
        }
    }

    /// What the tab is called.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Elements => "Elements",
            Self::Frame => "Frame",
            Self::Timeline => "Timeline",
            Self::Reactivity => "Reactivity",
            Self::Parity => "Parity",
            Self::Memory => "Memory",
        }
    }
}

/// Which tree the Tree tab is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TreeMode {
    /// Only the components, nested the way they were written.
    ///
    /// The default, because it is the tree somebody actually wrote: a program is a few dozen
    /// components and a few thousand nodes, and the first question about a document is almost
    /// always about the former.
    #[default]
    Components,
    /// Every node, with the component boundaries still marked among them.
    ///
    /// What the components turned into, which is where a question about a box nobody wrote gets
    /// answered — and the boundaries stay in it so the answer says which component wrote it.
    Full,
}

impl TreeMode {
    /// Both trees, in the order the toggle lists them.
    pub const ALL: [Self; 2] = [Self::Components, Self::Full];

    /// What the toggle calls it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Components => "Components",
            Self::Full => "All nodes",
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
    /// How long each of the last few frames took, in microseconds, oldest first.
    ///
    /// One number per frame rather than one per stage: the strip beside it says where a frame's
    /// time went, and this says which frames were the expensive ones — a spike is invisible in a
    /// breakdown of the frame that happens to be showing.
    pub(crate) history: RwSignal<Vec<f64>, LocalStorage>,
    /// How wide the docked panel is, in CSS pixels.
    ///
    /// Held here rather than in the panel's own body so it survives the panel being closed and
    /// opened again: a width somebody dragged to is a setting, and one that reverted every time
    /// F12 was pressed twice would be a worse default than the fixed width it replaced.
    pub(crate) width: RwSignal<f64, LocalStorage>,
    /// The document as the tree tab draws it.
    ///
    /// Behind an [`Rc`] because it is the one sample big enough for the difference to matter: it
    /// is cloned out of the signal by every closure that reads it, and a tree of a few thousand
    /// rows cloned per read is the panel becoming the thing it is measuring.
    pub(crate) tree: RwSignal<Option<Rc<Tree>>, LocalStorage>,
    /// Which tree the tree tab is showing.
    pub(crate) tree_mode: RwSignal<TreeMode, LocalStorage>,
    /// Which rows are open.
    ///
    /// Kept across resamples, which is what makes the tab usable at all: the tree is rebuilt
    /// whenever the document changes, and a set of open rows that went with it would collapse the
    /// whole tree every time anything moved.
    pub(crate) expanded: RwSignal<HashSet<NodeId>, LocalStorage>,
    /// The component the picked node was built by, when it was built by one.
    ///
    /// What the tree selects when the pointer picked something the tree is not showing: the
    /// components view has a row per boundary and none per element, so picking a box in the
    /// application would otherwise select nothing anybody can see.
    pub(crate) picked_component: RwSignal<Option<NodeId>, LocalStorage>,
    /// What is outlined in the application.
    ///
    /// An element, or a component's open marker — a component's outline is the union of what its
    /// content occupies, which is a question about the marker's extent rather than about a box.
    pub(crate) highlighted: RwSignal<Option<NodeId>, LocalStorage>,
    /// Where that outline goes, in CSS pixels, or nothing when there is nothing to outline.
    pub(crate) highlight_box: RwSignal<Option<Rect<CssPx, Css>>, LocalStorage>,
    /// The panel's own column, so what belongs to the inspector can be told from what does not.
    pub(crate) panel: RwSignal<Option<NodeId>, LocalStorage>,
    /// The application's own column, which is where the tree starts.
    pub(crate) app: RwSignal<Option<NodeId>, LocalStorage>,
    /// How tall the detail half of the elements tab is, in CSS pixels.
    pub(crate) detail: RwSignal<f64, LocalStorage>,
    /// What the reactive graph is holding.
    pub(crate) reactive: RwSignal<Reactive, LocalStorage>,
    /// The outline the inspector draws over the application.
    ///
    /// Kept apart from the panel because it is not inside it: it is portalled onto an overlay
    /// layer, so it is the inspector's and sits somewhere else entirely.
    pub(crate) overlay: RwSignal<Option<NodeId>, LocalStorage>,
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
            history: RwSignal::new_local(Vec::new()),
            width: RwSignal::new_local(DEFAULT_WIDTH),
            tree: RwSignal::new_local(None),
            tree_mode: RwSignal::new_local(TreeMode::default()),
            expanded: RwSignal::new_local(HashSet::new()),
            picked_component: RwSignal::new_local(None),
            highlighted: RwSignal::new_local(None),
            highlight_box: RwSignal::new_local(None),
            panel: RwSignal::new_local(None),
            app: RwSignal::new_local(None),
            detail: RwSignal::new_local(DEFAULT_DETAIL),
            reactive: RwSignal::new_local(Reactive::default()),
            overlay: RwSignal::new_local(None),
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

    /// What is picked.
    #[must_use]
    pub fn picked(&self) -> Option<NodeId> {
        self.picked.get_untracked()
    }

    /// The component the picked node was built by, when it was built by one.
    #[must_use]
    pub fn picked_component(&self) -> Option<NodeId> {
        self.picked_component.get_untracked()
    }

    /// Which tree the tree tab is showing.
    #[must_use]
    pub fn tree_mode(&self) -> TreeMode {
        self.tree_mode.get_untracked()
    }

    /// Shows `mode` in the tree tab.
    pub fn set_tree_mode(&self, mode: TreeMode) {
        self.tree_mode.set(mode);
    }

    /// How wide the docked panel is, in CSS pixels.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.width.get_untracked()
    }

    /// Asks for the panel to be `width` CSS pixels wide.
    ///
    /// Clamped to what the window allows the next time the panel lays out, exactly as a drag is —
    /// so a program may ask for anything and get something sensible.
    pub fn set_width(&self, width: f64) {
        let wanted = width.max(MIN_WIDTH);
        if self.width.get_untracked() != wanted {
            self.width.set(wanted);
        }
    }

    /// Opens or closes the tree row for `node`.
    ///
    /// Rows start open, so this is how a row is folded away as well as how one is put back.
    pub fn set_expanded(&self, node: NodeId, open: bool) {
        self.expanded.update(|shut| {
            if open {
                shut.remove(&node);
            } else {
                shut.insert(node);
            }
        });
    }

    /// Outlines `node` in the application, or nothing.
    ///
    /// What hovering a row in the tree does, offered here as well so a program can point the
    /// inspector at something from its own code.
    pub fn set_highlighted(&self, node: Option<NodeId>) {
        if self.highlighted.get_untracked() != node {
            self.highlighted.set(node);
        }
    }
}

/// How tall the detail half of the elements tab is before anybody drags it, in CSS pixels.
///
/// Enough for the selector, the box model and the first few computed properties — which is what a
/// reader wants immediately after picking something, with the tree still holding the room to find
/// the next thing.
pub(crate) const DEFAULT_DETAIL: f64 = 260.0;

/// The least either half of the elements tab is allowed to be, in CSS pixels.
pub(crate) const MIN_HALF: f64 = 80.0;

/// How wide the panel is before anybody drags it, in CSS pixels.
///
/// Wide enough for the tab strip to sit on one line with its icons, and for a computed-style
/// listing's longest property name to sit beside its value. The panel is read far more often than
/// it is dragged, and a default that needed dragging before it could be read is the wrong one.
pub(crate) const DEFAULT_WIDTH: f64 = 560.0;

/// The narrowest the panel goes, in CSS pixels.
///
/// Under this the two-column rows stop being two columns and every value wraps, so a drag that went
/// further would be destroying the panel rather than resizing it.
pub(crate) const MIN_WIDTH: f64 = 280.0;

/// How much of the window the application keeps, in CSS pixels, however far the panel is dragged.
///
/// The panel is a tool for looking at something, and a drag that covered the whole window would
/// leave nothing to look at — and no way back, because the divider would be off the edge.
pub(crate) const MIN_APP: f64 = 160.0;

/// How many latency marks are kept for the timeline.
///
/// A frame writes on the order of twenty, so this is a few hundred frames — far more than the strip
/// shows, and deliberately: the marks of the frame being *drawn* are still being written while the
/// strip is built, so what it renders is the frame before, and a ring that held exactly one frame
/// would always be showing a half-written one.
const RING: usize = 4096;
