//! The engine seam: resolved geometry, scrolling, focus, selection, animation and time.
//!
//! [`Dom`](crate::Dom) is the node tree. This is everything a view can ask of, or command in, the
//! engine that laid that tree out — and none of it is a question the node tree can answer. Where
//! a node's box actually ended up, what is focused, what is selected, how many animations are
//! running, and what should happen half a second from now are all facts about a running engine.
//!
//! Splitting them from `Dom` keeps both traits small and keeps each one implementable on its own:
//! a backend can bring up its node tree first and answer geometry with nothing until it has a
//! layout engine.

mod focus;
mod handle;
mod shortcut;
mod timer;

use core::ops::Range;
use core::time::Duration;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_reactive::{LocalStorage, Signal};

use crate::id::NodeId;
use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};

pub use crate::host::focus::{FocusMove, FocusTrap, FocusTrapId, FocusTrapOptions};
pub use crate::host::handle::HostHandle;
pub use crate::host::shortcut::WindowShortcut;
pub use crate::host::timer::{Repeat, TimerId};

/// What a view can ask of, or command in, the engine that laid its tree out.
///
/// Every geometry answer is *as of the last completed frame*. Reading layout in the middle of a
/// build cannot be made both correct and cheap, and a framework that pretends otherwise is a
/// framework that thrashes layout. A view that needs to react to geometry as it changes registers
/// an observation through [`Dom::observe`](crate::Dom::observe) instead.
///
/// ```
/// use std::rc::Rc;
/// use zgui_view::stub::StubHost;
/// use zgui_view::{DocumentId, NodeId, ViewHost};
///
/// let host: Rc<dyn ViewHost> = Rc::new(StubHost::default());
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
///
/// host.focus(node);
/// assert_eq!(host.running_animations(node), 0);
/// ```
pub trait ViewHost {
    /// The union of this node's boxes **relative to its parent's border box**, as of the last
    /// completed frame.
    ///
    /// The space is the one thing to keep hold of: this is where the box sits *inside its parent*,
    /// which is the answer to "how big is it" and "where in this row did it end up", and is not
    /// where it is on the screen. Anything comparing a box with a pointer — which reports where it
    /// is in the window — wants [`ViewHost::window_box`] instead.
    ///
    /// `None` when the node has no box: it has not been laid out yet, or it is not displayed.
    fn border_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>>;

    /// The same box in the **window's** coordinate space, as of the last completed frame.
    ///
    /// Every ancestor's origin is already summed into it, every ancestor's scroll offset already
    /// taken off and every transform on it or above it already applied, so this is the rectangle a
    /// pointer position can be compared with directly — the same space the hit test resolves in,
    /// and the place on the screen a person is looking at. It is what a control measuring a gesture
    /// against itself has to ask for: a slider working out how far along its track a press landed,
    /// a resizer, a drag. It is also what a surface placed against a trigger asks, which is why the
    /// transform belongs in it: a menu opened from inside a panel that has been moved by one goes
    /// where the trigger is, not where the trigger would have been.
    ///
    /// A transformed rectangle is not a rectangle, so a rotated box answers with the smallest
    /// upright box containing it. Anything that has to work in a turned box's *own* space — the
    /// space its text is laid out in — wants [`ViewHost::border_box`] and the matrix, not this.
    ///
    /// Still device pixels, so a pointer's position has to be multiplied by [`ViewHost::scale`]
    /// before the two are subtracted.
    ///
    /// `None` on the same terms as [`ViewHost::border_box`], and additionally for a node whose
    /// boxes did not survive to a fragment — one inside a subtree that was skipped.
    fn window_box(&self, node: NodeId) -> Option<Rect<DevicePx, Device>>;

    /// How many device pixels one CSS pixel is on this window's surface.
    ///
    /// A pointer event reports where it is in **CSS** pixels; [`ViewHost::window_box`] reports
    /// where an element is in **device** pixels, in the same origin. Anything that relates the two
    /// — a slider working out how far along its track a press landed, a resizer, a drag — needs
    /// the number that converts them, and a component that assumed it was one would be right on a
    /// desktop and wrong by a factor of two on the machine beside it.
    ///
    /// Answers as of the last completed frame, like everything else here.
    fn scale(&self) -> f32;

    /// This scroll container's offset, content extent and visible extent.
    fn scroll_position(&self, node: NodeId) -> ScrollPosition;

    /// Asks for a scroll.
    ///
    /// The scroll happens in the frame this schedules, not before this call returns, so reading
    /// [`ViewHost::scroll_position`] immediately afterwards still reports the old offset.
    fn scroll_to(&self, node: NodeId, target: ScrollTarget, behavior: ScrollBehavior);

    /// Stops, or lets go of, this window's own scrolling.
    ///
    /// A frozen window is still scrolled exactly as far as it was: the offset it holds, the width
    /// its content wrapped to and the gutter its scrollbar occupies are all left alone, and every
    /// pixel stays where it was. What stops is *movement* — a wheel, a trackpad, a key, an
    /// accessibility action and a scroll asked for by a view all leave the window where it is.
    ///
    /// This is what a modal surface holds while it is open, and the distinction is the whole point
    /// of it. Taking the window's scrolling away by restyling it — `overflow: hidden` on the root —
    /// clamps the offset to the top, so the page jumps under the modal and jumps back when it
    /// closes. Freezing changes nothing about the layout and therefore moves nothing.
    ///
    /// Only the window's own scrolling is frozen. A scroll container inside the page keeps its own,
    /// which is what the surface opened over the page needs: its own content still scrolls.
    ///
    /// Calls do not nest: `true` twice is the same as `true` once. A caller that opens surfaces
    /// inside surfaces keeps its own count and freezes on the first hold and thaws on the last.
    fn freeze_scrolling(&self, frozen: bool);

    /// Moves focus to this node.
    fn focus(&self, node: NodeId);

    /// The node that currently holds focus in this window, as a reactive value.
    ///
    /// An implementation creates this signal **once, when the window is created, under the
    /// window's root scope**, and returns clones of it. Minting it lazily inside whichever scope
    /// happens to call first makes every other caller's copy die when that one caller unmounts,
    /// which is a panic in code that never did anything wrong.
    fn focused(&self) -> Signal<Option<NodeId>, LocalStorage>;

    /// Whether `other` is `ancestor` or sits inside it.
    fn contains(&self, ancestor: NodeId, other: NodeId) -> bool;

    /// Whether `first` comes before `second` in tree order.
    ///
    /// Tree order is the order a depth-first walk of the document reaches nodes in, which is the
    /// order a reader meets them and the order a composite control has to present its items in. A
    /// node never precedes itself, and a node that is no longer in the tree precedes nothing.
    ///
    /// This exists because registration order is not tree order: a list whose rows are keyed and
    /// reordered registers its items in whatever order the rows were last rebuilt, and a menu
    /// whose items arrived that way would answer the down-arrow with the wrong one.
    fn precedes(&self, first: NodeId, second: NodeId) -> bool;

    /// Every focusable node in this subtree, in sequential focus-navigation order, as of the last
    /// completed frame.
    ///
    /// A snapshot rather than an iterator: the source of truth sits behind the document's own
    /// batched mutation, and handing component code something that holds a borrow across
    /// arbitrary calls is how that becomes a re-entrancy bug.
    fn focusables(&self, root: NodeId) -> Vec<NodeId>;

    /// Moves focus within this subtree, returning the node that received it.
    fn focus_move(&self, root: NodeId, direction: FocusMove) -> Option<NodeId>;

    /// Confines sequential focus navigation to `root`'s subtree.
    ///
    /// Reach for [`NodeRef::trap_focus`](crate::NodeRef::trap_focus) instead, which pairs this
    /// with the guard that undoes it.
    fn push_focus_trap(&self, root: NodeId, options: FocusTrapOptions) -> FocusTrapId;

    /// Removes an installed trap.
    fn pop_focus_trap(&self, id: FocusTrapId);

    /// Registers `node` to hear keys that nothing in the window has focus for.
    ///
    /// A key is delivered along the path to whatever holds focus, and a window in which nothing
    /// holds focus — the state every window launches in, and returns to whenever focus is dropped
    /// — has a path one element long: the document's root. So a chord written as an ordinary
    /// listener is dead until somebody clicks or tabs, wherever in the tree it sits, and it is
    /// dead again the moment focus goes away.
    ///
    /// A registration made here is delivered to after that path, so the node's own listeners hear
    /// the key wherever the node sits. Only its own: this names one element, not a subtree and not
    /// a path, so nothing between the root and it is put on the route.
    ///
    /// Registering a node that is already registered registers it once.
    ///
    /// Reach for [`NodeRef::window_shortcut`](crate::NodeRef::window_shortcut) instead, which
    /// pairs this with the removal an unmounting view owes.
    fn add_window_shortcut(&self, node: NodeId);

    /// Removes a registration. Removing one that was never made does nothing.
    fn remove_window_shortcut(&self, node: NodeId);

    /// The selection in this editable node, in document offsets.
    fn selection(&self, node: NodeId) -> Option<Range<usize>>;

    /// Replaces the selection in this editable node.
    fn set_selection(&self, node: NodeId, range: Range<usize>);

    /// Selects everything in this editable node.
    fn select_all(&self, node: NodeId);

    /// Puts `text` in this editable node, as the value its application owns.
    ///
    /// This is what makes a field bound to a signal possible. An editable node's text is not
    /// something a view writes — the framework keeps an editing model over it, holding the caret,
    /// the undo stack and any composition in progress, and a view that replaced the text nodes
    /// underneath would leave the model typing into text that is no longer there. So the value goes
    /// through here, the model takes it, and the model writes it out.
    ///
    /// Safe to call on every change of the signal driving it, which is how a controlled field is
    /// written:
    ///
    /// - text the node already holds does nothing whatsoever, so the echo of the user's own
    ///   keystroke — announced as an [`Input`](zgui_vocab::ValueChange::Input) event, written to a
    ///   signal, arriving back here — does not throw away the caret they are typing at;
    /// - text that really is different keeps the caret where it was, clamped into the new text,
    ///   which is what an application that transforms what it was told needs;
    /// - it applies to a disabled or read-only node too, because that is its application's value
    ///   rather than something a person is typing.
    ///
    /// It reports nothing back: a value an application wrote is not news to that application, so no
    /// [`Input`](zgui_vocab::ValueChange::Input) event is dispatched for it. What the user does
    /// with the field afterwards is announced in the ordinary way.
    ///
    /// Like everything else that writes here, this is carried out in the frame it schedules rather
    /// than before the call returns.
    fn set_value(&self, node: NodeId, text: &str);

    /// How many animations and transitions are currently running on this node.
    fn running_animations(&self, node: NodeId) -> usize;

    /// Runs `callback` on the user-interface thread once `after` has elapsed, or every `after`
    /// when `repeat` says so.
    ///
    /// The deadline is measured against the engine's own clock, never the wall clock, so a test
    /// harness that advances time by hand fires these exactly as a running window does. The
    /// callback runs at the start of a frame, before that frame's reactive work, so anything it
    /// writes settles in the same frame it fired in.
    ///
    /// Reach for [`set_timeout`](crate::time::set_timeout) or
    /// [`set_interval`](crate::time::set_interval) instead: they pair this with the cancellation
    /// a callback that outlives its scope needs.
    fn schedule(&self, after: Duration, repeat: Repeat, callback: Rc<dyn Fn()>) -> TimerId;

    /// Cancels a scheduled callback. Cancelling one that already fired, or one that was already
    /// cancelled, does nothing.
    fn cancel_timer(&self, timer: TimerId);

    /// Installs `css` as a style sheet of this document's, under `name`.
    ///
    /// The sheet goes to the author origin, over the framework's own sheet and under nothing, so a
    /// component library's rules are ordinary author rules that an application's own sheet can
    /// out-specify or override in the ordinary way.
    ///
    /// Installing under a name that is already installed **replaces that sheet's text and keeps
    /// its place in the cascade**. Removing and adding instead would move the sheet to the end of
    /// its origin, where it would start winning against every sheet that used to beat it — which
    /// is why a theme that changes re-installs rather than reinstalls. Installing text identical
    /// to what is already there is a no-op, so a component that installs its own sheet from every
    /// instance's body pays for one.
    ///
    /// Reach for [`install_stylesheet`](crate::install_stylesheet) instead, which resolves the
    /// host from the enclosing window.
    fn install_stylesheet(&self, name: &str, css: &str);

    /// Removes the sheet installed under `name`. Removing one that is not installed does nothing.
    fn remove_stylesheet(&self, name: &str);
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::ViewHost;
    use crate::stub::StubHost;
    use crate::{DocumentId, NodeId};

    #[test]
    fn the_trait_is_object_safe() {
        let host: Rc<dyn ViewHost> = Rc::new(StubHost::default());
        let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
        assert_eq!(host.focusables(node), Vec::new());
    }
}
