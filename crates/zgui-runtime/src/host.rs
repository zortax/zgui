//! The engine seam a view asks its geometry, its focus and its timers through.
//!
//! Everything here answers **as of the last completed frame**, and that is not a limitation to be
//! worked around. Reading layout in the middle of a build cannot be made both correct and cheap; a
//! framework that pretends otherwise is a framework that lays out twice per component. A view that
//! has to react to geometry as it changes registers an observation instead, and the frame delivers
//! it after layout has settled and before anything is painted.
//!
//! Commands go the other way. A handler asking for focus, for a scroll, or for a pointer capture
//! is running while the document is mid-change, so nothing may take effect where it is asked for.
//! Requests are appended here and carried out by the frame that follows.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui_dom::{Document, NodeKey};
use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::LayoutStore;
use zgui_reactive::{LocalStorage, Signal};
use zgui_view::host::{FocusMove, FocusTrapId, FocusTrapOptions, Repeat, TimerId};
use zgui_view::{DocumentId, NodeId, ScrollBehavior, ScrollPosition, ScrollTarget, ViewHost};

mod entering;

pub use crate::host::entering::{ATTEMPTS, Entering, Owed};

use crate::timer::Timers;
use crate::wake::RuntimeWaker;

/// Something a view asked for that the next frame carries out.
///
/// It is a queue rather than an immediate effect because every one of these can be issued from
/// inside an event handler, and an event handler runs inside an open batch of changes to the
/// document. Applying a focus move there would re-enter a change that has not finished.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Move focus to this node, or take focus away when there is none.
    Focus(Option<NodeId>),
    /// Scroll this container.
    Scroll {
        /// The node to scroll.
        node: NodeId,
        /// Where to scroll it to.
        target: ScrollTarget,
        /// Whether to animate getting there.
        behavior: ScrollBehavior,
    },
    /// Route subsequent pointer events to this node until the button is released.
    CapturePointer(NodeId),
    /// Install this style sheet at the author origin, or replace it when the name is taken.
    InstallStylesheet {
        /// What the sheet is installed under.
        name: String,
        /// Its text.
        css: String,
    },
    /// Remove the style sheet installed under this name.
    RemoveStylesheet {
        /// What it was installed under.
        name: String,
    },
    /// Stop, or let go of, the window's own scrolling, leaving it exactly where it is.
    FreezeScrolling(bool),
    /// End a capture early.
    ReleasePointer(NodeId),
    /// Put this node's own editing model's selection here.
    ///
    /// Recording the range is not enough on its own. An editable element's caret lives in its
    /// editing model, which is what the next keystroke replaces text at, so a selection written
    /// from a view has to reach the model as well as the record of it — otherwise selecting a
    /// field's contents and typing over them leaves the old text where it was.
    Select {
        /// The node whose selection this is.
        node: NodeId,
        /// The range, in offsets into that node's own text.
        range: Range<usize>,
    },
    /// Put this node's own editing model's text here.
    ///
    /// The value of a field an application owns. It reaches the model rather than the document
    /// because the model is what holds the caret, the undo stack and any composition, and text
    /// written past it would leave all three describing text that is no longer there.
    SetValue {
        /// The node whose value this is.
        node: NodeId,
        /// The text it should hold.
        text: String,
    },
    /// Dispatch an event on this node through the ordinary capture, target and bubble path.
    Synthesize {
        /// The node to aim it at.
        node: NodeId,
        /// Which event.
        event: zgui_vocab::EventKind,
    },
}

/// The production [`ViewHost`]: the running engines, seen from a view.
///
/// One per window, shared with the window's frame loop. Everything it reads is behind a shared
/// cell because a view can ask a question from anywhere — including from inside an observation
/// delivery, which happens in the middle of a frame — and everything it writes is a command the
/// frame drains.
pub struct RuntimeHost {
    /// Which window this is.
    document_id: DocumentId,
    /// The document.
    document: Rc<RefCell<Document>>,
    /// The boxes, their results and their fragments, as of the last completed frame.
    layout: Rc<RefCell<LayoutStore>>,
    /// Where each scroll container is scrolled to.
    scroll: Rc<RefCell<zgui_scroll::Scroller>>,
    /// The scheduled callbacks of every window.
    timers: Rc<RefCell<Timers>>,
    /// What the next frame has been asked to do.
    commands: RefCell<Vec<Command>>,
    /// Which node holds focus, as a reactive value.
    ///
    /// Created once, with the window, under the window's own scope. Minting it inside whichever
    /// scope happened to ask first would make every other holder's copy die when that one caller
    /// unmounted, which is a panic in code that did nothing wrong.
    focused: Signal<Option<NodeId>, LocalStorage>,
    /// Where the focus signal is written from.
    focused_writer: zgui_reactive::RwSignal<Option<NodeId>, LocalStorage>,
    /// How many animations and transitions were running on each element as of the last completed
    /// frame.
    ///
    /// Published by the frame rather than read from the style engine on demand, for the reason
    /// every other geometry answer here is: the engine is mid-change while a view is asking, and
    /// an answer taken from it there is an answer about a frame that has not happened.
    animations: RefCell<rustc_hash::FxHashMap<NodeKey, usize>>,
    /// The matrices the last composed frame's fragments are drawn under.
    ///
    /// Held beside the fragments they belong to for the same reason: a fragment records its
    /// rectangle in its own untransformed space and names its matrix by an index, so where a box
    /// *is* is only answerable from the two together.
    placements: RefCell<zgui_scene::Placements>,
    /// The text of every style sheet a view has installed, by name.
    ///
    /// Kept here so that installing a sheet that is already installed with the same text issues no
    /// command and therefore asks for no frame. A component installs its own sheet from every
    /// instance's body, and a list of two hundred rows would otherwise wake the loop two hundred
    /// times to replace a sheet with itself.
    sheets: RefCell<rustc_hash::FxHashMap<String, String>>,
    /// What is selected in each editable node.
    ///
    /// Held here rather than in the document because a selection is state about the document and
    /// not part of it: it survives a restyle, it is not inherited, and nothing below the runtime
    /// may keep a second copy of it.
    selections: crate::selection::Selections,
    /// The traps installed, innermost last.
    ///
    /// The stack itself is the input system's, not a second one kept here: which subtree confines
    /// traversal, and where focus goes back to when a trap is removed, are questions about focus,
    /// and answering them twice is how a dialog comes to restore focus to one place while the
    /// traversal it confined believed another.
    traps: RefCell<zgui_input::focus::FocusTraps>,
    /// The elements registered to hear keys nothing in the window has focus for.
    ///
    /// Keys rather than view identities, because what reads this is the routing that resolves a
    /// plan against the document, and a registration for a node that has since been removed
    /// resolves to no listener rather than to whichever node took its place.
    shortcuts: RefCell<Vec<NodeKey>>,
    /// The traps that have asked for the focus and are still waiting for it.
    ///
    /// See [`entering`]: a trap goes up while the surface it confines is being built, which is
    /// before that surface has a single box, and the focus it asks for has nowhere to land until
    /// the frame has laid it out.
    entering: RefCell<Entering>,
    /// Where a request for a frame goes.
    ///
    /// Issuing a command means the next frame has work to do, and there is no guarantee that
    /// anything else is going to ask for that frame: a command issued from a resolved future or
    /// from a callback the platform raised outside a frame has nobody behind it. Routing through
    /// the same waker the reactive layer uses gets both cases right at once — folded into the
    /// frame's single request when one is running, and a platform ping when none is.
    waker: Arc<RuntimeWaker>,
    /// Where the clock comes from.
    clock: Arc<dyn zgui_platform::Clock>,
    /// How many device pixels one CSS pixel is on this window's surface.
    ///
    /// Written by the frame that learns of a change rather than read from the surface on demand,
    /// for the reason every other answer here is published rather than pulled: a view asking
    /// mid-frame would get the number the frame is in the middle of changing to.
    scale: std::cell::Cell<f32>,
}

impl RuntimeHost {
    /// Builds the host for one window.
    ///
    /// Call inside the window's own reactive scope: the focus signal is created here and must
    /// belong to the window rather than to whatever built the first view.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document_id: DocumentId,
        document: Rc<RefCell<Document>>,
        layout: Rc<RefCell<LayoutStore>>,
        scroll: Rc<RefCell<zgui_scroll::Scroller>>,
        timers: Rc<RefCell<Timers>>,
        waker: Arc<RuntimeWaker>,
        clock: Arc<dyn zgui_platform::Clock>,
    ) -> Self {
        let focused_writer = zgui_reactive::RwSignal::new_local(None);
        Self {
            document_id,
            document,
            layout,
            scroll,
            timers,
            commands: RefCell::new(Vec::new()),
            animations: RefCell::default(),
            placements: RefCell::new(zgui_scene::Placements::new()),
            sheets: RefCell::default(),
            selections: crate::selection::Selections::new(),
            focused: focused_writer.into(),
            focused_writer,
            traps: RefCell::new(zgui_input::focus::FocusTraps::default()),
            shortcuts: RefCell::default(),
            entering: RefCell::default(),
            waker,
            clock,
            scale: std::cell::Cell::new(1.0),
        }
    }

    /// Publishes how many device pixels one CSS pixel is, for the frames that follow.
    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    /// Appends a command for the next frame to carry out.
    pub fn issue(&self, command: Command) {
        self.commands.borrow_mut().push(command);
        // A command means something changed, so the frame that carries it out has to be asked for.
        // Inside a frame that request is folded into the one the frame's last phase makes.
        use zgui_reactive::FrameWaker;
        self.waker.wake();
    }

    /// Records what is selected in a node, in the offsets its own text is measured in.
    ///
    /// The editing model's route in: it has just changed the document and knows exactly what is
    /// selected, so the range is written as it stands rather than clamped against a length read
    /// back out of the tree it is in the middle of changing.
    pub fn write_selection(&self, node: NodeKey, range: Range<usize>) {
        let length = range.end;
        self.selections.set(node, range, length);
    }

    /// What is selected in a node, by the document's own name for it.
    ///
    /// The counterpart of [`RuntimeHost::write_selection`], for the frame rather than for a view: a
    /// field that is focused again has to put its caret back where it was left, and the record is
    /// the only thing that remembers.
    pub fn selection_of(&self, node: NodeKey) -> Option<Range<usize>> {
        self.selections.of(node)
    }

    /// Takes everything asked for since the last call.
    pub fn take_commands(&self) -> Vec<Command> {
        core::mem::take(&mut *self.commands.borrow_mut())
    }

    /// Publishes which node holds focus, so anything reading it updates.
    pub fn publish_focus(&self, node: Option<NodeKey>) {
        use zgui_reactive::prelude::Set;
        self.focused_writer
            .set(node.map(zgui_view_dom::id::to_view));
    }

    /// Publishes how many animations each element is running, as of the frame that just styled.
    ///
    /// Called once per frame with every element the style engine holds animations for. An element
    /// that is absent is running none, so the map is replaced rather than merged.
    pub fn publish_animations(&self, counts: rustc_hash::FxHashMap<NodeKey, usize>) {
        *self.animations.borrow_mut() = counts;
    }

    /// Publishes where this frame's coordinate systems ended up.
    ///
    /// A fragment names the coordinate system it is drawn in and keeps its rectangle in that
    /// system's own coordinates, so the two have to come with each other for either to mean
    /// anything: geometry answered from matrices that belong to a different frame is geometry
    /// about a picture nobody saw.
    ///
    /// What is kept here is the tree's *answers* rather than a copy of the tree. A name is
    /// structural and means the same coordinate system in every frame, so nothing has to be copied
    /// to keep a name from a frame ago meaningful — what changes between frames is where the
    /// coordinate system is, which is one matrix per coordinate system and not one per distinct
    /// matrix in the document. A thousand identical rows resolve to one.
    ///
    /// Every coordinate system that resolves to something other than what it did is listed into
    /// `moved`, when a caller asks for them. This is the only moment at which both answers exist,
    /// and a reader that filed something under a name — a rectangle handed to an assistive
    /// technology, which lives outside this process and cannot re-read it — has no other way to
    /// find out that what it filed no longer describes the screen: the name it holds is the same
    /// name, and nothing about the element it names has changed.
    ///
    /// Asking costs one comparison per coordinate system per frame, so it is the caller's decision
    /// and not this one's. Everything inside the process reads a matrix when it wants one and is
    /// never stale; the only reader that cannot is one that was *sent* a rectangle, and until a
    /// frame has sent one there is nothing to correct.
    pub fn publish_placements(
        &self,
        spatial: &zgui_scene::SpatialTree,
        moved: Option<&mut Vec<zgui_scene::SpatialId>>,
    ) {
        let mut placements = self.placements.borrow_mut();
        match moved {
            Some(moved) => placements.take_noting_moves(spatial, &mut |id| moved.push(id)),
            None => placements.take(spatial),
        }
    }

    /// Where the coordinate systems the last composed frame's fragments name ended up.
    ///
    /// The frame's own path to them, for the geometry it delivers to whoever is watching a node: it
    /// must be answered from the same matrices a view asking later is answered from, or an observer
    /// and a direct reader would disagree about where the same box is.
    pub(crate) fn placements(&self) -> std::cell::Ref<'_, zgui_scene::Placements> {
        self.placements.borrow()
    }

    /// The elements registered to hear a key nothing in the window has focus for.
    ///
    /// Read once per unfocused key press by the routing that resolves the plan, and empty on every
    /// window that has not asked for one — which is the ordinary case, and is why this allocates
    /// nothing until something registers.
    pub fn window_shortcuts(&self) -> Vec<NodeKey> {
        self.shortcuts.borrow().clone()
    }

    /// The innermost installed focus trap, if there is one.
    ///
    /// Traps over subtrees that have left the document are uninstalled on the way past. This is
    /// the one place anything asks which trap is in force, so it is the one place that can notice.
    pub fn active_trap(&self) -> Option<(NodeId, FocusTrapOptions)> {
        self.drop_stranded_traps();
        self.traps.borrow().topmost().map(|trap| {
            let mut options = FocusTrapOptions::MODAL;
            options.wrap = trap.options.wrap;
            options.auto_focus = trap.options.auto_focus;
            options.restore = trap.options.restore;
            (zgui_view_dom::id::to_view(trap.root), options)
        })
    }

    /// Uninstalls every trap whose surface has left the document, and puts focus back for them.
    ///
    /// A surface takes its trap down as it goes, and that is how one ends. This is for the surface
    /// that went without doing so: what it leaves behind confines the keyboard to a subtree that
    /// no longer exists, which is a window where no key moves focus anywhere, ever again. Focus is
    /// restored exactly as it would have been by an orderly removal, and only to a node that is
    /// itself still there.
    fn drop_stranded_traps(&self) {
        let stranded = {
            let document = self.document.borrow();
            let mut traps = self.traps.borrow_mut();
            let stranded = traps.drop_stranded(document.store());
            stranded
                .into_iter()
                .map(|trap| {
                    let restore = trap
                        .restore_to
                        .filter(|node| document.store().index_of(*node).is_some());
                    (trap.id, trap.options.restore, restore)
                })
                .collect::<Vec<_>>()
        };
        for (id, restore, node) in stranded {
            self.entering
                .borrow_mut()
                .forget(FocusTrapId::new(id.get()));
            if restore {
                self.issue(Command::Focus(node.map(zgui_view_dom::id::to_view)));
            }
        }
    }

    /// Whether any self-focusing trap is still waiting to be entered.
    pub fn owes_focus(&self) -> bool {
        !self.entering.borrow().is_empty()
    }

    /// Asks for the focus every self-focusing trap is owed, now that there is a layout to find it
    /// in.
    ///
    /// Called by the frame after layout. Each ask is skipped when the trap it came from has since
    /// been removed, and when the focus is already inside the subtree — a surface whose own content
    /// took the caret as it appeared has already been entered, and moving to its first element
    /// would drag the caret off whatever that content chose.
    pub fn enter_owed_traps(&self) {
        // Taken into a local first: the loop below borrows the same cell again to carry an ask
        // over, and a borrow held across the iteration would be the second one.
        let asks = self.entering.borrow_mut().take();
        for owed in asks {
            if !self
                .traps
                .borrow()
                .holds(zgui_input::FocusTrapId::new(owed.trap.get()))
            {
                continue;
            }
            let inside = {
                use zgui_reactive::prelude::GetUntracked;
                self.focused
                    .get_untracked()
                    .is_some_and(|node| self.contains(owed.root, node))
            };
            if inside {
                continue;
            }
            if self.focus_move(owed.root, FocusMove::First).is_some() {
                continue;
            }
            // Nothing inside it can be focused *yet*: a surface that floats beside its trigger is
            // laid out hidden once so that it can be measured before it is placed. The ask is
            // carried to the next frame, and that frame is asked for here because the measurement
            // may be the only other thing that wanted one.
            if self.entering.borrow_mut().carry(owed) {
                use zgui_reactive::FrameWaker;
                self.waker.wake();
            }
        }
    }

    /// How many bytes of text a node's subtree holds, or zero when it names no live node.
    fn text_length(&self, key: NodeKey) -> usize {
        let document = self.document.borrow();
        match document.store().index_of(key) {
            Some(index) => crate::selection::text_length(document.store(), index),
            None => 0,
        }
    }

    /// The document's own name for a view's handle, when it still names a live node.
    fn key_of(&self, node: NodeId) -> Option<NodeKey> {
        let document = self.document.borrow();
        let key = zgui_view_dom::id::to_document(node)?;
        document.store().index_of(key).map(|_| key)
    }
}

impl ViewHost for RuntimeHost {
    fn border_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        let key = self.key_of(node)?;
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(key).first()?;
        layout
            .layout_of(first)
            .map(|resolved| resolved.border_box())
    }

    fn window_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        let key = self.key_of(node)?;
        let layout = self.layout.borrow();
        let first = *layout.boxes_of(key).first()?;
        zgui_layout::fragment::transform::placed::window_box(
            &layout,
            first,
            &self.placements.borrow(),
        )
    }

    fn scale(&self) -> f32 {
        self.scale.get()
    }

    fn scroll_position(&self, node: NodeId) -> ScrollPosition {
        let Some(key) = self.key_of(node) else {
            return ScrollPosition::default();
        };
        let layout = self.layout.borrow();
        let Some(first) = layout.boxes_of(key).first().copied() else {
            return ScrollPosition::default();
        };
        let offset = self.scroll.borrow().offset_of(key);
        let Some(region) = zgui_layout::scroll_region::region_of(&layout, first) else {
            return ScrollPosition::default();
        };
        ScrollPosition {
            offset,
            content_size: region.content,
            scrollport: region.scrollport.size,
        }
    }

    fn scroll_to(&self, node: NodeId, target: ScrollTarget, behavior: ScrollBehavior) {
        self.issue(Command::Scroll {
            node,
            target,
            behavior,
        });
    }

    fn freeze_scrolling(&self, frozen: bool) {
        self.issue(Command::FreezeScrolling(frozen));
    }

    fn focus(&self, node: NodeId) {
        self.issue(Command::Focus(Some(node)));
    }

    fn focused(&self) -> Signal<Option<NodeId>, LocalStorage> {
        self.focused
    }

    fn contains(&self, ancestor: NodeId, other: NodeId) -> bool {
        let (Some(ancestor), Some(other)) = (self.key_of(ancestor), self.key_of(other)) else {
            return false;
        };
        if ancestor == other {
            return true;
        }
        let document = self.document.borrow();
        zgui_input::HitChain::to_root(document.store(), other).contains(ancestor)
    }

    fn focusables(&self, root: NodeId) -> Vec<NodeId> {
        let Some(root) = self.key_of(root) else {
            return Vec::new();
        };
        let document = self.document.borrow();
        let layout = self.layout.borrow();
        zgui_input::focus::order::focusables(document.store(), Some(&layout), root)
            .into_iter()
            .map(zgui_view_dom::id::to_view)
            .collect()
    }

    fn focus_move(&self, root: NodeId, direction: FocusMove) -> Option<NodeId> {
        let sequence = self.focusables(root);
        let keys: Vec<NodeKey> = sequence
            .iter()
            .filter_map(|node| self.key_of(*node))
            .collect();
        let current = {
            use zgui_reactive::prelude::GetUntracked;
            self.focused
                .get_untracked()
                .and_then(|node| self.key_of(node))
        };
        let wrap = self.active_trap().is_some_and(|(_, options)| options.wrap);
        let moved = zgui_input::focus::order::step(&keys, current, translate(direction), wrap)?;
        let node = zgui_view_dom::id::to_view(moved);
        self.issue(Command::Focus(Some(node)));
        Some(node)
    }

    fn push_focus_trap(&self, root: NodeId, options: FocusTrapOptions) -> FocusTrapId {
        let Some(key) = self.key_of(root) else {
            // A trap over a node that no longer exists confines nothing. Installing it would leave
            // a stack entry whose subtree no traversal can reach, which is a window that cannot be
            // tabbed at all until it is removed again.
            return FocusTrapId::new(0);
        };
        // Where focus is *now*, so that removing the trap can put it back. Read here rather than at
        // removal for the reason the whole mechanism exists: by then focus is inside the dialog.
        let held = {
            use zgui_reactive::prelude::GetUntracked;
            self.focused
                .get_untracked()
                .and_then(|node| self.key_of(node))
        };
        let id = self.traps.borrow_mut().push(
            key,
            zgui_input::TrapOptions {
                wrap: options.wrap,
                auto_focus: options.auto_focus,
                restore: options.restore,
            },
            held,
        );
        let id = FocusTrapId::new(id.get());
        if options.auto_focus {
            // Recorded rather than done. The surface this trap confines was mounted moments ago
            // and has not been laid out, so it has no boxes and nothing inside it can be focused
            // yet; the frame pays this after layout. See [`Entering`].
            self.entering.borrow_mut().owe(id, root);
            // Nothing else in this frame necessarily wants one, and a menu that opened without a
            // frame to enter it in is a menu the keyboard never reaches.
            use zgui_reactive::FrameWaker;
            self.waker.wake();
        }
        id
    }

    fn add_window_shortcut(&self, node: NodeId) {
        let Some(key) = self.key_of(node) else {
            return;
        };
        let mut shortcuts = self.shortcuts.borrow_mut();
        if !shortcuts.contains(&key) {
            shortcuts.push(key);
        }
    }

    fn remove_window_shortcut(&self, node: NodeId) {
        let Some(key) = self.key_of(node) else {
            return;
        };
        self.shortcuts.borrow_mut().retain(|held| *held != key);
    }

    fn pop_focus_trap(&self, id: FocusTrapId) {
        self.entering.borrow_mut().forget(id);
        let removed = self
            .traps
            .borrow_mut()
            .pop(zgui_input::FocusTrapId::new(id.get()));
        let Some(trap) = removed else {
            return;
        };
        if !trap.options.restore {
            return;
        }
        // Focus goes back to whatever opened the dialog. Dropping it instead leaves the whole
        // window unfocused, and the next key press goes to the document rather than to the control
        // the person was on before they opened anything.
        self.issue(Command::Focus(
            trap.restore_to.map(zgui_view_dom::id::to_view),
        ));
    }

    fn selection(&self, node: NodeId) -> Option<Range<usize>> {
        self.selections.of(self.key_of(node)?)
    }

    fn set_selection(&self, node: NodeId, range: Range<usize>) {
        let Some(key) = self.key_of(node) else {
            return;
        };
        let length = self.text_length(key);
        self.selections.set(key, range, length);
        // The model has to hear about it too, because the caret it types at is its own. Issued
        // rather than written: the model is the window's, and this is called from inside handlers
        // that run in the middle of a frame.
        //
        // What is selected changed, so the frame that shows it is owed. Nothing is marked on the
        // document here: this is called from inside handlers, which run inside an open batch of
        // changes, and the frame that follows is where a caret and a highlight are drawn.
        self.issue(Command::Select {
            node,
            range: self.selections.of(key).unwrap_or(0..0),
        });
    }

    fn select_all(&self, node: NodeId) {
        let Some(key) = self.key_of(node) else {
            return;
        };
        let length = self.text_length(key);
        self.selections.set(key, 0..length, length);
        self.issue(Command::Select {
            node,
            range: 0..length,
        });
    }

    fn set_value(&self, node: NodeId, text: &str) {
        if self.key_of(node).is_none() {
            return;
        }
        // Nothing is decided here, not even whether the text differs from what the field holds:
        // the text of an editable element is the editing model's, the model is the window's, and
        // this is called from effects and handlers that run in the middle of a frame. The model
        // answers in the frame this asks for, and it is the model that leaves the caret alone when
        // the value it is handed is the one it already has.
        self.issue(Command::SetValue {
            node,
            text: text.to_owned(),
        });
    }

    fn running_animations(&self, node: NodeId) -> usize {
        let Some(key) = self.key_of(node) else {
            return 0;
        };
        self.animations.borrow().get(&key).copied().unwrap_or(0)
    }

    fn schedule(&self, after: Duration, repeat: Repeat, callback: Rc<dyn Fn()>) -> TimerId {
        self.timers.borrow_mut().schedule(
            self.document_id,
            self.clock.now(),
            after,
            repeat,
            callback,
        )
    }

    fn cancel_timer(&self, timer: TimerId) {
        self.timers.borrow_mut().cancel(timer);
    }

    fn precedes(&self, first: NodeId, second: NodeId) -> bool {
        if first == second {
            return false;
        }
        let (Some(first), Some(second)) = (self.key_of(first), self.key_of(second)) else {
            return false;
        };
        let document = self.document.borrow();
        crate::order::precedes(document.store(), first, second)
    }

    fn install_stylesheet(&self, name: &str, css: &str) {
        {
            let mut sheets = self.sheets.borrow_mut();
            if sheets.get(name).is_some_and(|installed| installed == css) {
                return;
            }
            sheets.insert(name.to_owned(), css.to_owned());
        }
        self.issue(Command::InstallStylesheet {
            name: name.to_owned(),
            css: css.to_owned(),
        });
    }

    fn remove_stylesheet(&self, name: &str) {
        if self.sheets.borrow_mut().remove(name).is_none() {
            return;
        }
        self.issue(Command::RemoveStylesheet {
            name: name.to_owned(),
        });
    }
}

/// The input system's name for a direction the view layer named.
///
/// Two enumerations rather than one because the view layer must not depend on the input system,
/// and both are closed sets that this one function keeps in step.
fn translate(direction: FocusMove) -> zgui_input::FocusDirection {
    match direction {
        FocusMove::First => zgui_input::FocusDirection::First,
        FocusMove::Last => zgui_input::FocusDirection::Last,
        FocusMove::Next => zgui_input::FocusDirection::Next,
        FocusMove::Prev => zgui_input::FocusDirection::Prev,
        // A direction this build has never heard of moves focus forwards, which is what every
        // traversal a component writes means by "onwards".
        _ => zgui_input::FocusDirection::Next,
    }
}
