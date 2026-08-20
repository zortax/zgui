//! A host with the geometry a test declared, recording what the view asked of it.

use core::ops::Range;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_reactive::{LocalStorage, Signal};
use zgui_view::stub::StubHost;
use zgui_view::{
    FocusMove, FocusTrapId, FocusTrapOptions, FrameRequestId, NodeId, Repeat, ScrollBehavior,
    ScrollPosition, ScrollTarget, TimerId, Timestamp, ViewHost,
};

use crate::transcript::{Op, Transcript};

/// A [`ViewHost`] whose answers a test declares and whose commands go into a transcript.
///
/// Geometry has to come from somewhere, and in a component test it cannot come from a layout
/// engine — there is not one. It comes from the test instead: it says where the boxes are, and the
/// component is then asserted on for what it did with that. Everything the component *asks for* —
/// focus, scroll, a trap, a selection — is recorded into the same transcript the node tree writes
/// into, so a claim about order is answerable.
///
/// ```
/// use std::rc::Rc;
/// use zgui_testkit_view::ScriptedHost;
/// use zgui_view::{DocumentId, NodeId, ViewHost};
///
/// let host = ScriptedHost::new();
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
///
/// host.focus(node);
/// assert_eq!(host.transcript().to_string(), "focus #1\n");
/// ```
pub struct ScriptedHost {
    /// What answers the questions and keeps the clock.
    inner: StubHost,
    /// What has been asked of it.
    transcript: Transcript,
    /// Values an application has written into fields, waiting for a frame to carry them out.
    ///
    /// Queued rather than applied where the call is made, because that is what a window does: the
    /// text of an editable element belongs to the editing model, the model is the window's, and
    /// [`ViewHost::set_value`] is reached from effects and handlers running in the middle of a
    /// frame. A harness that wrote through immediately would let a component pass while relying on
    /// an ordering no window offers.
    written: RefCell<Vec<(NodeId, String)>>,
}

impl Default for ScriptedHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptedHost {
    /// A host that knows nothing yet, with its clock at the origin.
    pub fn new() -> Self {
        Self::with_transcript(Transcript::new())
    }

    /// The same, recording into a transcript something else is also recording into.
    pub fn with_transcript(transcript: Transcript) -> Self {
        Self {
            inner: StubHost::new(),
            transcript,
            written: RefCell::new(Vec::new()),
        }
    }

    /// What has been asked of this host.
    pub fn transcript(&self) -> Transcript {
        self.transcript.clone()
    }

    /// Every value written into a field since this was last asked, in order.
    ///
    /// What a frame carries out. Draining it is what closes the loop of a controlled field.
    pub fn take_written_values(&self) -> Vec<(NodeId, String)> {
        core::mem::take(&mut self.written.borrow_mut())
    }

    /// Records where an edit left the caret.
    ///
    /// Not [`ViewHost::set_selection`]: that is a view *asking* for a selection and belongs in the
    /// transcript, while this is the framework reporting where its own model ended up. A harness
    /// that conflated them would show a component demanding a selection it never asked for.
    pub fn write_selection(&self, node: NodeId, range: Range<usize>) {
        ViewHost::set_selection(&self.inner, node, range);
    }

    /// Declares a node's border box, which is what a scripted press is aimed with.
    /// Declares how many device pixels one CSS pixel is.
    pub fn set_scale(&self, scale: f32) {
        self.inner.set_scale(scale);
    }

    /// Declares a node's border box.
    pub fn set_border_box(&self, node: NodeId, bounds: Rect<DevicePx, Device>) {
        self.inner.set_border_box(node, bounds);
    }

    /// Declares a node's scroll position.
    pub fn set_scroll_position(&self, node: NodeId, position: ScrollPosition) {
        self.inner.set_scroll_position(node, position);
    }

    /// Declares the focusable nodes inside a subtree.
    pub fn set_focusables(&self, root: NodeId, focusables: Vec<NodeId>) {
        self.inner.set_focusables(root, focusables);
    }

    /// Declares that `descendant` sits inside `ancestor`.
    pub fn set_contains(&self, ancestor: NodeId, descendant: NodeId) {
        self.inner.set_contains(ancestor, descendant);
    }

    /// Declares the document's tree order, which is what [`ViewHost::precedes`] answers from.
    pub fn set_tree_order(&self, order: Vec<NodeId>) {
        self.inner.set_tree_order(order);
    }

    /// Whether the view has frozen this window's own scrolling.
    pub fn scrolling_frozen(&self) -> bool {
        self.inner.scrolling_frozen()
    }

    /// The text of the style sheet installed under `name`.
    pub fn stylesheet(&self, name: &str) -> Option<String> {
        self.inner.stylesheet(name)
    }

    /// The names of the installed style sheets, in the order they were first installed.
    pub fn stylesheet_names(&self) -> Vec<String> {
        self.inner.stylesheet_names()
    }

    /// How many style sheets are installed.
    pub fn stylesheet_count(&self) -> usize {
        self.inner.stylesheet_count()
    }

    /// How many installs actually changed a sheet.
    pub fn stylesheet_installs(&self) -> usize {
        self.inner.stylesheet_installs()
    }

    /// Declares how many animations are running on a node.
    pub fn set_running_animations(&self, node: NodeId, count: usize) {
        self.inner.set_running_animations(node, count);
    }

    /// The scroll commands the view has issued, in order.
    pub fn scroll_commands(&self) -> Vec<(NodeId, ScrollTarget, ScrollBehavior)> {
        self.inner.scroll_commands()
    }

    /// How many focus traps are installed.
    pub fn live_focus_traps(&self) -> usize {
        self.inner.live_focus_traps()
    }

    /// The innermost installed trap.
    pub fn topmost_focus_trap(&self) -> Option<(FocusTrapId, NodeId, FocusTrapOptions)> {
        self.inner.topmost_focus_trap()
    }

    /// How many callbacks are still scheduled.
    pub fn live_timers(&self) -> usize {
        self.inner.live_timers()
    }

    /// How far the virtual clock has advanced from its origin.
    pub fn now(&self) -> Duration {
        self.inner.now()
    }

    /// Moves the clock forward, firing everything that comes due, in deadline order.
    pub fn advance(&self, by: Duration) {
        self.inner.advance(by);
    }
}

impl core::fmt::Debug for ScriptedHost {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ScriptedHost")
            .field("now", &self.inner.now())
            .field("timers", &self.inner.live_timers())
            .field("traps", &self.inner.live_focus_traps())
            .finish_non_exhaustive()
    }
}

impl ViewHost for ScriptedHost {
    fn border_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        self.inner.border_box(node)
    }

    fn window_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        self.inner.window_box(node)
    }

    fn scale(&self) -> f32 {
        self.inner.scale()
    }

    fn scroll_position(&self, node: NodeId) -> ScrollPosition {
        self.inner.scroll_position(node)
    }

    fn scroll_to(&self, node: NodeId, target: ScrollTarget, behavior: ScrollBehavior) {
        self.transcript.push(Op::Scroll { node });
        self.inner.scroll_to(node, target, behavior);
    }

    fn freeze_scrolling(&self, frozen: bool) {
        self.inner.freeze_scrolling(frozen);
    }

    fn focus(&self, node: NodeId) {
        self.transcript.push(Op::Focus { node });
        self.inner.focus(node);
    }

    fn focused(&self) -> Signal<Option<NodeId>, LocalStorage> {
        self.inner.focused()
    }

    fn contains(&self, ancestor: NodeId, other: NodeId) -> bool {
        self.inner.contains(ancestor, other)
    }

    fn focusables(&self, root: NodeId) -> Vec<NodeId> {
        self.inner.focusables(root)
    }

    fn focus_move(&self, root: NodeId, direction: FocusMove) -> Option<NodeId> {
        let moved = self.inner.focus_move(root, direction);
        if let Some(node) = moved {
            self.transcript.push(Op::Focus { node });
        }
        moved
    }

    fn push_focus_trap(&self, root: NodeId, options: FocusTrapOptions) -> FocusTrapId {
        self.transcript.push(Op::PushFocusTrap { node: root });
        self.inner.push_focus_trap(root, options)
    }

    fn pop_focus_trap(&self, id: FocusTrapId) {
        self.transcript.push(Op::PopFocusTrap);
        self.inner.pop_focus_trap(id);
    }

    fn add_window_shortcut(&self, node: NodeId) {
        self.inner.add_window_shortcut(node);
    }

    fn remove_window_shortcut(&self, node: NodeId) {
        self.inner.remove_window_shortcut(node);
    }

    fn selection(&self, node: NodeId) -> Option<Range<usize>> {
        self.inner.selection(node)
    }

    fn set_selection(&self, node: NodeId, range: Range<usize>) {
        self.transcript.push(Op::SetSelection {
            node,
            start: range.start,
            end: range.end,
        });
        self.inner.set_selection(node, range);
    }

    fn select_all(&self, node: NodeId) {
        self.transcript.push(Op::SelectAll { node });
        self.inner.select_all(node);
    }

    fn set_value(&self, node: NodeId, text: &str) {
        // Recorded only when it changed something, which is the contract a controlled field is
        // written against: the echo of the user's own keystroke is a call that must do nothing, and
        // a transcript that showed a line for it would be asserting the opposite behaviour.
        let before = self.inner.value(node);
        self.inner.set_value(node, text);
        if self.inner.value(node) != before {
            self.transcript.push(Op::SetValue {
                node,
                text: text.to_owned(),
            });
        }
        self.written.borrow_mut().push((node, text.to_owned()));
    }

    fn running_animations(&self, node: NodeId) -> usize {
        self.inner.running_animations(node)
    }

    fn schedule(&self, after: Duration, repeat: Repeat, callback: Rc<dyn Fn()>) -> TimerId {
        self.inner.schedule(after, repeat, callback)
    }

    fn cancel_timer(&self, timer: TimerId) {
        self.inner.cancel_timer(timer);
    }

    fn request_frame_callback(&self, callback: Rc<dyn Fn(Timestamp)>) -> FrameRequestId {
        self.inner.request_frame_callback(callback)
    }

    fn cancel_frame_callback(&self, request: FrameRequestId) {
        self.inner.cancel_frame_callback(request);
    }

    fn precedes(&self, first: NodeId, second: NodeId) -> bool {
        self.inner.precedes(first, second)
    }

    fn install_stylesheet(&self, name: &str, css: &str) {
        let before = self.inner.stylesheet_installs();
        self.inner.install_stylesheet(name, css);
        // Only a install that changed something is recorded. A component that installs its own
        // sheet from every instance's body would otherwise fill a transcript with the same line.
        if self.inner.stylesheet_installs() != before {
            self.transcript.push(Op::InstallStylesheet {
                name: name.to_owned(),
            });
        }
    }

    fn remove_stylesheet(&self, name: &str) {
        self.transcript.push(Op::RemoveStylesheet {
            name: name.to_owned(),
        });
        self.inner.remove_stylesheet(name);
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;
    use std::cell::Cell;
    use std::rc::Rc;

    use zgui_view::{DocumentId, FocusTrapOptions, NodeId, Repeat, ViewHost};

    use super::ScriptedHost;

    fn node(raw: u64) -> NodeId {
        NodeId::new(DocumentId::FIRST, raw).expect("in range")
    }

    #[test]
    fn the_commands_a_view_issues_land_in_the_transcript_in_order() {
        let host = ScriptedHost::new();
        let trap = host.push_focus_trap(node(1), FocusTrapOptions::MODAL);
        host.focus(node(2));
        host.select_all(node(2));
        host.pop_focus_trap(trap);

        assert_eq!(
            host.transcript().to_string(),
            "trap #1\nfocus #2\nselect-all #2\nuntrap\n"
        );
    }

    #[test]
    fn a_delay_costs_a_test_no_real_time_at_all() {
        let host = ScriptedHost::new();
        let fired = Rc::new(Cell::new(false));
        let flag = Rc::clone(&fired);
        host.schedule(
            Duration::from_millis(700),
            Repeat::Once,
            Rc::new(move || flag.set(true)),
        );

        host.advance(Duration::from_millis(699));
        assert!(!fired.get(), "not yet");
        host.advance(Duration::from_millis(1));
        assert!(fired.get(), "and now");
        assert_eq!(host.live_timers(), 0);
    }
}
