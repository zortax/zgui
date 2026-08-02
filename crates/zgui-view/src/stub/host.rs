//! An engine that reports whatever a test told it to report.

use core::cell::{Cell, RefCell};
use core::ops::Range;
use core::time::Duration;
use std::collections::BTreeMap;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_reactive::prelude::*;
use zgui_reactive::{LocalStorage, RwSignal, Signal};

use crate::host::{FocusMove, FocusTrapId, FocusTrapOptions, Repeat, TimerId, ViewHost};
use crate::id::NodeId;
use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};

/// One scheduled callback.
struct Scheduled {
    /// Which registration this is.
    id: TimerId,
    /// When it is next due, measured from the clock's origin.
    due: Duration,
    /// How long it waits between runs.
    every: Duration,
    /// Whether it runs again after firing.
    repeat: Repeat,
    /// What runs.
    callback: Rc<dyn Fn()>,
}

/// A [`ViewHost`] with no engine behind it.
///
/// Every geometry answer is whatever a test put there, and every command is recorded rather than
/// carried out. Its clock is virtual: [`StubHost::advance`] is the only thing that makes time
/// pass, so a test of a delayed behaviour is exact rather than nearly always right.
///
/// ```
/// use core::time::Duration;
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui_view::stub::StubHost;
/// use zgui_view::{Repeat, ViewHost};
///
/// let host = StubHost::default();
/// let fired = Rc::new(Cell::new(0));
///
/// let counter = Rc::clone(&fired);
/// host.schedule(Duration::from_millis(700), Repeat::Once, Rc::new(move || {
///     counter.set(counter.get() + 1);
/// }));
///
/// host.advance(Duration::from_millis(699));
/// assert_eq!(fired.get(), 0);
/// host.advance(Duration::from_millis(1));
/// assert_eq!(fired.get(), 1);
/// ```
pub struct StubHost {
    /// What holds focus, as a reactive value, created once here.
    focused: RwSignal<Option<NodeId>, LocalStorage>,
    /// The border boxes a test declared.
    boxes: RefCell<BTreeMap<NodeId, Rect<DevicePx, Device>>>,
    /// The scroll positions a test declared.
    scrolls: RefCell<BTreeMap<NodeId, ScrollPosition>>,
    /// The scroll commands the view issued.
    scroll_commands: RefCell<Vec<(NodeId, ScrollTarget, ScrollBehavior)>>,
    /// Whether the window's own scrolling is frozen.
    frozen: Cell<bool>,
    /// The focusable sets a test declared, by subtree root.
    focusables: RefCell<BTreeMap<NodeId, Vec<NodeId>>>,
    /// The containment a test declared, as `(ancestor, descendant)` pairs.
    containment: RefCell<Vec<(NodeId, NodeId)>>,
    /// The traps that are installed, innermost last.
    traps: RefCell<Vec<(FocusTrapId, NodeId, FocusTrapOptions)>>,
    /// The nodes registered to hear keys nothing has focus for.
    shortcuts: RefCell<Vec<NodeId>>,
    /// The next trap number to mint.
    next_trap: Cell<u64>,
    /// The selections a test declared.
    selections: RefCell<BTreeMap<NodeId, Range<usize>>>,
    /// The text each editable node holds, as a test declared it or a view loaded it.
    values: RefCell<BTreeMap<NodeId, String>>,
    /// How many animations a test declared per node.
    animations: RefCell<BTreeMap<NodeId, usize>>,
    /// The scheduled callbacks that have not fired or been cancelled.
    timers: RefCell<Vec<Scheduled>>,
    /// The next timer number to mint.
    next_timer: Cell<u64>,
    /// How far the virtual clock has advanced from its origin.
    now: Cell<Duration>,
    /// The style sheets installed, by name, in installation order.
    sheets: RefCell<Vec<(String, String)>>,
    /// How many installs actually changed a sheet.
    ///
    /// Counted rather than derived, because "installing the same text twice costs nothing" is a
    /// promise about the calls that reach the engine and the final map cannot show it.
    sheet_installs: Cell<usize>,
    /// The tree order a test declared, if it declared one.
    tree_order: RefCell<Option<Vec<NodeId>>>,
    /// How many device pixels one CSS pixel is, which is one until a test says otherwise.
    scale: Cell<f32>,
}

impl Default for StubHost {
    fn default() -> Self {
        Self::new()
    }
}

impl StubHost {
    /// A host that knows nothing yet, with its clock at the origin.
    pub fn new() -> Self {
        Self {
            focused: RwSignal::new_local(None),
            boxes: RefCell::default(),
            scrolls: RefCell::default(),
            scroll_commands: RefCell::default(),
            frozen: Cell::new(false),
            focusables: RefCell::default(),
            containment: RefCell::default(),
            traps: RefCell::default(),
            shortcuts: RefCell::default(),
            next_trap: Cell::new(1),
            selections: RefCell::default(),
            values: RefCell::default(),
            animations: RefCell::default(),
            timers: RefCell::default(),
            next_timer: Cell::new(1),
            now: Cell::new(Duration::ZERO),
            sheets: RefCell::default(),
            sheet_installs: Cell::new(0),
            tree_order: RefCell::default(),
            scale: Cell::new(1.0),
        }
    }

    /// Declares the document's tree order, which is what [`ViewHost::precedes`] answers from.
    ///
    /// With none declared, handles are compared by the number they were minted with. A stub tree
    /// mints them in creation order and a view creates its nodes in the order it writes them, so
    /// that is tree order for everything built straight through — but it is *not* tree order once
    /// a keyed list has reordered its rows, which is exactly the case the method exists for. A
    /// test about ordering declares the order rather than relying on the fallback.
    pub fn set_tree_order(&self, order: Vec<NodeId>) {
        *self.tree_order.borrow_mut() = Some(order);
    }

    /// The text of the sheet installed under `name`.
    pub fn stylesheet(&self, name: &str) -> Option<String> {
        self.sheets
            .borrow()
            .iter()
            .find(|(installed, _)| installed == name)
            .map(|(_, css)| css.clone())
    }

    /// How many sheets are installed.
    pub fn stylesheet_count(&self) -> usize {
        self.sheets.borrow().len()
    }

    /// The names of the installed sheets, in the order they were first installed.
    pub fn stylesheet_names(&self) -> Vec<String> {
        self.sheets
            .borrow()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// How many installs changed something.
    pub fn stylesheet_installs(&self) -> usize {
        self.sheet_installs.get()
    }

    /// Declares how many device pixels one CSS pixel is.
    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    /// Declares a node's border box.
    pub fn set_border_box(&self, node: NodeId, bounds: Rect<DevicePx, Device>) {
        self.boxes.borrow_mut().insert(node, bounds);
    }

    /// Declares a node's scroll position.
    pub fn set_scroll_position(&self, node: NodeId, position: ScrollPosition) {
        self.scrolls.borrow_mut().insert(node, position);
    }

    /// Declares the focusable nodes inside a subtree.
    pub fn set_focusables(&self, root: NodeId, focusables: Vec<NodeId>) {
        self.focusables.borrow_mut().insert(root, focusables);
    }

    /// Declares that `descendant` sits inside `ancestor`.
    pub fn set_contains(&self, ancestor: NodeId, descendant: NodeId) {
        self.containment.borrow_mut().push((ancestor, descendant));
    }

    /// Declares how many animations are running on a node.
    pub fn set_running_animations(&self, node: NodeId, count: usize) {
        self.animations.borrow_mut().insert(node, count);
    }

    /// The text an editable node holds, which is what a view last loaded into it.
    pub fn value(&self, node: NodeId) -> Option<String> {
        self.values.borrow().get(&node).cloned()
    }

    /// The scroll commands the view has issued, in order.
    pub fn scroll_commands(&self) -> Vec<(NodeId, ScrollTarget, ScrollBehavior)> {
        self.scroll_commands.borrow().clone()
    }

    /// Whether the view has frozen this window's own scrolling.
    pub fn scrolling_frozen(&self) -> bool {
        self.frozen.get()
    }

    /// How many window shortcuts are registered.
    pub fn live_window_shortcuts(&self) -> usize {
        self.shortcuts.borrow().len()
    }

    /// How many focus traps are installed.
    pub fn live_focus_traps(&self) -> usize {
        self.traps.borrow().len()
    }

    /// The innermost installed trap.
    pub fn topmost_focus_trap(&self) -> Option<(FocusTrapId, NodeId, FocusTrapOptions)> {
        self.traps.borrow().last().copied()
    }

    /// How many callbacks are still scheduled.
    pub fn live_timers(&self) -> usize {
        self.timers.borrow().len()
    }

    /// How far the virtual clock has advanced from its origin.
    pub fn now(&self) -> Duration {
        self.now.get()
    }

    /// Moves the clock forward, firing everything that comes due, in deadline order.
    ///
    /// A repeating callback that comes due more than once inside one advance fires once per
    /// deadline, which is what a frame loop that missed a frame does.
    pub fn advance(&self, by: Duration) {
        let target = self.now.get() + by;
        loop {
            let next = self
                .timers
                .borrow()
                .iter()
                .filter(|timer| timer.due <= target)
                .map(|timer| (timer.due, timer.id))
                .min();
            let Some((due, id)) = next else {
                break;
            };
            self.now.set(due);
            let callback = {
                let mut timers = self.timers.borrow_mut();
                let Some(at) = timers.iter().position(|timer| timer.id == id) else {
                    continue;
                };
                let callback = Rc::clone(&timers[at].callback);
                if timers[at].repeat.is_repeating() {
                    let every = timers[at].every;
                    timers[at].due = due + every.max(Duration::from_nanos(1));
                } else {
                    timers.remove(at);
                }
                callback
            };
            callback();
        }
        self.now.set(target);
    }
}

impl ViewHost for StubHost {
    fn border_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        self.boxes.borrow().get(&node).copied()
    }

    fn window_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        // One box per node and no tree to accumulate through: whatever was recorded is both the
        // parent-relative answer and the window one. A fixture that needs the two to differ records
        // the window-relative box, because that is the one a control does arithmetic with.
        self.boxes.borrow().get(&node).copied()
    }

    fn scale(&self) -> f32 {
        self.scale.get()
    }

    fn scroll_position(&self, node: NodeId) -> ScrollPosition {
        self.scrolls
            .borrow()
            .get(&node)
            .copied()
            .unwrap_or_default()
    }

    fn scroll_to(&self, node: NodeId, target: ScrollTarget, behavior: ScrollBehavior) {
        self.scroll_commands
            .borrow_mut()
            .push((node, target, behavior));
    }

    fn freeze_scrolling(&self, frozen: bool) {
        self.frozen.set(frozen);
    }

    fn focus(&self, node: NodeId) {
        self.focused.set(Some(node));
    }

    fn focused(&self) -> Signal<Option<NodeId>, LocalStorage> {
        self.focused.into()
    }

    fn contains(&self, ancestor: NodeId, other: NodeId) -> bool {
        ancestor == other
            || self
                .containment
                .borrow()
                .iter()
                .any(|(outer, inner)| *outer == ancestor && *inner == other)
    }

    fn focusables(&self, root: NodeId) -> Vec<NodeId> {
        self.focusables
            .borrow()
            .get(&root)
            .cloned()
            .unwrap_or_default()
    }

    fn focus_move(&self, root: NodeId, direction: FocusMove) -> Option<NodeId> {
        let focusables = self.focusables(root);
        if focusables.is_empty() {
            return None;
        }
        let current = self
            .focused
            .get_untracked()
            .and_then(|node| focusables.iter().position(|candidate| *candidate == node));
        let at = match (direction, current) {
            (FocusMove::First, _) => 0,
            (FocusMove::Last, _) => focusables.len() - 1,
            (FocusMove::Next, Some(at)) => (at + 1) % focusables.len(),
            (FocusMove::Next, None) => 0,
            (FocusMove::Prev, Some(at)) => (at + focusables.len() - 1) % focusables.len(),
            (FocusMove::Prev, None) => focusables.len() - 1,
        };
        let node = focusables[at];
        self.focused.set(Some(node));
        Some(node)
    }

    fn push_focus_trap(&self, root: NodeId, options: FocusTrapOptions) -> FocusTrapId {
        let id = FocusTrapId::new(self.next_trap.get());
        self.next_trap.set(self.next_trap.get() + 1);
        self.traps.borrow_mut().push((id, root, options));
        id
    }

    fn pop_focus_trap(&self, id: FocusTrapId) {
        self.traps
            .borrow_mut()
            .retain(|(installed, _, _)| *installed != id);
    }

    fn add_window_shortcut(&self, node: NodeId) {
        let mut shortcuts = self.shortcuts.borrow_mut();
        if !shortcuts.contains(&node) {
            shortcuts.push(node);
        }
    }

    fn remove_window_shortcut(&self, node: NodeId) {
        self.shortcuts.borrow_mut().retain(|held| *held != node);
    }

    fn selection(&self, node: NodeId) -> Option<Range<usize>> {
        self.selections.borrow().get(&node).cloned()
    }

    fn set_selection(&self, node: NodeId, range: Range<usize>) {
        self.selections.borrow_mut().insert(node, range);
    }

    fn select_all(&self, node: NodeId) {
        self.selections.borrow_mut().insert(node, 0..usize::MAX);
    }

    fn set_value(&self, node: NodeId, text: &str) {
        // The stub keeps the one rule a component's behaviour actually depends on: loading the text
        // that is already there leaves the caret alone. A stub that stored unconditionally would
        // let a component that re-loads its field on every keystroke pass here and lose the caret
        // in a window.
        let mut values = self.values.borrow_mut();
        let held = values.entry(node).or_default();
        if held == text {
            return;
        }
        text.clone_into(held);
        if let Some(selection) = self.selections.borrow_mut().get_mut(&node) {
            let start = selection.start.min(text.len());
            *selection = start..selection.end.clamp(start, text.len());
        }
    }

    fn running_animations(&self, node: NodeId) -> usize {
        self.animations
            .borrow()
            .get(&node)
            .copied()
            .unwrap_or_default()
    }

    fn schedule(&self, after: Duration, repeat: Repeat, callback: Rc<dyn Fn()>) -> TimerId {
        let id = TimerId::new(self.next_timer.get());
        self.next_timer.set(self.next_timer.get() + 1);
        self.timers.borrow_mut().push(Scheduled {
            id,
            due: self.now.get() + after,
            every: after,
            repeat,
            callback,
        });
        id
    }

    fn cancel_timer(&self, timer: TimerId) {
        self.timers
            .borrow_mut()
            .retain(|scheduled| scheduled.id != timer);
    }

    fn precedes(&self, first: NodeId, second: NodeId) -> bool {
        if first == second {
            return false;
        }
        match self.tree_order.borrow().as_ref() {
            Some(order) => {
                let at = |node: NodeId| order.iter().position(|candidate| *candidate == node);
                match (at(first), at(second)) {
                    (Some(first), Some(second)) => first < second,
                    // A node that is not in the declared order is not in the tree, and a node that
                    // is not in the tree precedes nothing.
                    _ => false,
                }
            }
            None => first.backend_bits() < second.backend_bits(),
        }
    }

    fn install_stylesheet(&self, name: &str, css: &str) {
        let mut sheets = self.sheets.borrow_mut();
        match sheets.iter_mut().find(|(installed, _)| installed == name) {
            Some((_, text)) if text == css => {}
            Some((_, text)) => {
                *text = css.to_owned();
                self.sheet_installs.set(self.sheet_installs.get() + 1);
            }
            None => {
                sheets.push((name.to_owned(), css.to_owned()));
                self.sheet_installs.set(self.sheet_installs.get() + 1);
            }
        }
    }

    fn remove_stylesheet(&self, name: &str) {
        self.sheets
            .borrow_mut()
            .retain(|(installed, _)| installed != name);
    }
}

#[cfg(test)]
mod tests {
    use crate::host::ViewHost;
    use crate::id::{DocumentId, NodeId};
    use crate::stub::StubHost;

    /// A node to load a value into.
    fn node() -> NodeId {
        NodeId::new(DocumentId::FIRST, 1).expect("in range")
    }

    #[test]
    fn loading_the_value_a_node_already_holds_leaves_its_caret_alone() {
        // The rule a controlled field is written against, kept by the stub so that a component
        // tested against it behaves the same way in a window: the echo of the user's own keystroke
        // must not disturb the caret they are typing at.
        let host = StubHost::default();
        let field = node();
        host.set_value(field, "abc");
        host.set_selection(field, 1..1);

        host.set_value(field, "abc");
        assert_eq!(host.selection(field), Some(1..1));

        // A value that really is different keeps the caret too, clamped into the text that is now
        // there — an offset past the end is a panic in a caller that slices one with the other.
        host.set_value(field, "ab");
        assert_eq!(host.value(field).as_deref(), Some("ab"));
        assert_eq!(host.selection(field), Some(1..1));
        host.set_value(field, "");
        assert_eq!(host.selection(field), Some(0..0));
    }
}
