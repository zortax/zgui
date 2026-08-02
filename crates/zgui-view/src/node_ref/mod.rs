//! A handle on a node a view made, and the imperative escape hatches it carries.
//!
//! Every geometry answer here is *as of the last completed frame*. Reading layout during a build
//! cannot be made both correct and cheap, and a framework that pretends otherwise is a framework
//! that thrashes layout. A view that has to react to geometry as it changes registers one of the
//! three observations instead, and gets a signal that is written during the frame that changed it,
//! before anything is painted.

mod listen;
mod observations;

use core::cell::Cell;
use core::ops::Range;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, LocalStorage, RenderEffect, RwSignal, Signal, on_cleanup_local};
use zgui_vocab::ListenerOptions;

use crate::cx::current_host;
use crate::dom::{DomHandle, Observed};
use crate::event::{EventCx, EventType, erase};
use crate::host::{FocusMove, FocusTrap, FocusTrapOptions, HostHandle, WindowShortcut};
use crate::id::NodeId;
use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};

pub use crate::node_ref::listen::ListenerGuard;
pub use crate::node_ref::observations::ObservationRegistry;

/// What a mounted `node_ref` binding records.
///
/// The two handles live **inside** the signal's value rather than beside it. Beside it they would
/// silently take [`Copy`] away from [`NodeRef`], because a reference count is not `Copy` — and
/// every component that stores a `NodeRef` in a `move` closure rests on it being `Copy`.
#[derive(Clone)]
struct Bound {
    /// The node.
    node: NodeId,
    /// The tree it belongs to, for registering observations.
    dom: DomHandle,
    /// The engine that can answer for it.
    host: HostHandle,
}

/// A handle to a node a view created, available once that view is built.
///
/// `Copy`, so it can be stored in any number of closures without ceremony, and every read is over
/// `try_get`: a `NodeRef` outlives its component trivially, so "the view has gone away" is an
/// ordinary answer here, never a panic.
///
/// ```
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{DocumentId, DomHandle, HostHandle, NodeRef};
/// use zgui_interned::ElementName;
///
/// install().unwrap();
/// let window = Mounted::new();
/// let dom = DomHandle::new(StubDom::new(DocumentId::FIRST));
/// let host = HostHandle::new(StubHost::default());
///
/// let node_ref = window.with(NodeRef::new);
/// assert_eq!(node_ref.get(), None, "nothing is bound yet");
///
/// let element = dom.create_element(ElementName::new("box"));
/// node_ref.bind(element, &dom, &host);
/// assert_eq!(node_ref.get(), Some(element));
///
/// window.unmount();
/// assert_eq!(node_ref.get(), None, "and reading it afterwards is not a panic");
/// ```
#[derive(Copy, Clone)]
pub struct NodeRef(RwSignal<Option<Bound>, LocalStorage>);

impl NodeRef {
    /// An unbound handle, belonging to the current reactive scope.
    pub fn new() -> Self {
        Self(RwSignal::new_local(None))
    }

    /// Binds this handle to `node`.
    ///
    /// A view calls this as it builds the element the handle was written on. Binding is a signal
    /// write, so anything waiting on the handle re-runs at the next flush.
    pub fn bind(&self, node: NodeId, dom: &DomHandle, host: &HostHandle) {
        self.0.try_set(Some(Bound {
            node,
            dom: dom.clone(),
            host: host.clone(),
        }));
    }

    /// Unbinds this handle, which is what a view does as it unmounts the element.
    pub fn unbind(&self) {
        self.0.try_set(None);
    }

    /// The node, once it exists — and `None` after the view that made it went away.
    pub fn get(&self) -> Option<NodeId> {
        self.0.try_get().flatten().map(|bound| bound.node)
    }

    /// The node, without subscribing to it.
    pub fn get_untracked(&self) -> Option<NodeId> {
        self.0.try_get_untracked().flatten().map(|bound| bound.node)
    }

    /// Whether this handle is bound to a node right now.
    pub fn is_bound(&self) -> bool {
        self.get_untracked().is_some()
    }

    /// What is bound, when anything is.
    fn bound(&self) -> Option<Bound> {
        self.0.try_get_untracked().flatten()
    }

    // ---- one-shot reads, from the last completed frame ------------------------------------

    /// The union of this node's boxes **relative to its parent's border box**, from the last
    /// completed frame.
    ///
    /// The size is what it is anywhere; the origin is where this element sits inside its parent and
    /// is *not* where it is on the screen. Comparing this origin with a pointer's position is the
    /// mistake — see [`NodeRef::window_bounds`], which answers in the space a pointer reports in.
    pub fn bounds(&self) -> Option<Rect<DevicePx, Device>> {
        let bound = self.bound()?;
        bound.host.border_box(bound.node)
    }

    /// The same box in the **window's** coordinate space, from the last completed frame.
    ///
    /// Every ancestor's origin summed in and every ancestor's scroll offset taken off, which makes
    /// this the rectangle a pointer position can be measured against. What a control asking *how
    /// far along myself did that press land* has to use.
    ///
    /// ```no_run
    /// # use zgui_view::NodeRef;
    /// # fn example(track: NodeRef, pointer_x_in_css_px: f32) -> Option<f32> {
    /// let track_box = track.window_bounds()?;
    /// let x = pointer_x_in_css_px * track.scale();
    /// Some((x - track_box.origin.x.0) / track_box.size.width.0)
    /// # }
    /// ```
    pub fn window_bounds(&self) -> Option<Rect<DevicePx, Device>> {
        let bound = self.bound()?;
        bound.host.window_box(bound.node)
    }

    /// How many device pixels one CSS pixel is on the surface this element is on.
    ///
    /// One outside a window, where there is no surface to ask. Inside one it is what converts a
    /// pointer event's position — which is in CSS pixels — into the space
    /// [`NodeRef::window_bounds`] answers in, which is the conversion every dragged control is
    /// built on.
    pub fn scale(&self) -> f32 {
        self.bound().map_or(1.0, |bound| bound.host.scale())
    }

    /// Every character this node's subtree contributes, in order.
    ///
    /// What a composite control matches a typed character against. A menu's typeahead asks *which
    /// item reads as starting with `p`?*, and the text an item reads as is the text it renders —
    /// written by whoever wrote the item, rather than repeated to the control as a second string
    /// that drifts from the first the moment either is edited.
    ///
    /// The empty string when this handle is not bound, and when the node holds no text.
    pub fn text_content(&self) -> String {
        self.bound()
            .map_or_else(String::new, |bound| bound.dom.text_content(bound.node))
    }

    /// This scroll container's offset, from the last completed frame.
    pub fn scroll_offset(&self) -> Point<DevicePx, Device> {
        self.scroll_position().offset
    }

    /// This scroll container's offset, content extent and visible extent, from the last completed
    /// frame.
    pub fn scroll_position(&self) -> ScrollPosition {
        self.bound()
            .map(|bound| bound.host.scroll_position(bound.node))
            .unwrap_or_default()
    }

    /// Asks for a scroll.
    pub fn scroll_to(&self, target: ScrollTarget, behavior: ScrollBehavior) {
        if let Some(bound) = self.bound() {
            bound.host.scroll_to(bound.node, target, behavior);
        }
    }

    /// Moves focus to this node.
    pub fn focus(&self) {
        if let Some(bound) = self.bound() {
            bound.host.focus(bound.node);
        }
    }

    /// Whether `other` is this node or sits inside it.
    ///
    /// `false` when this handle is not bound, which is the answer a dismissable overlay wants: a
    /// press cannot be inside something that is not there.
    pub fn contains(&self, other: NodeId) -> bool {
        self.bound()
            .is_some_and(|bound| bound.host.contains(bound.node, other))
    }

    /// Whether this node comes before `other` in tree order.
    ///
    /// What a set of items registered from anywhere is put back into document order with: a keyed
    /// list rebuilds its rows in whatever order the keys moved, so the order items announced
    /// themselves in is not the order a reader meets them in.
    pub fn precedes(&self, other: NodeId) -> bool {
        self.bound()
            .is_some_and(|bound| bound.host.precedes(bound.node, other))
    }

    // ---- the rest of the window ----------------------------------------------------------------

    /// The root element of the window this node is in.
    ///
    /// A view can only attach a listener to a node it made, so without a handle on the root there
    /// is no way to hear about a press *past* an open menu — which is the whole of dismissing one.
    /// The handle belongs to the calling scope and dies with it; the element does not.
    ///
    /// `None` when this handle is not bound, because there is no window to ask about.
    pub fn window_root(&self) -> Option<Self> {
        let bound = self.bound()?;
        let root = bound.dom.root(bound.node);
        let handle = Self::new();
        handle.bind(root, &bound.dom, &bound.host);
        Some(handle)
    }

    // ---- listening to a node this view did not create --------------------------------------

    /// Attaches `handler` to this node for as long as the returned guard is held.
    ///
    /// The declarative `on:` form is what a view uses for its own elements, and it is better in
    /// every way where it applies: the listener's lifetime is the element's, and nothing has to be
    /// stored. This is for the case it does not cover — a listener on a node the view did not
    /// create, reached through [`NodeRef::window_root`] — where the element outlives the view and
    /// the registration must therefore be undone explicitly.
    ///
    /// `None` when this handle is not bound.
    ///
    /// ```
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    /// use zgui_reactive::{Mounted, install};
    /// use zgui_view::stub::{StubDom, StubHost};
    /// use zgui_view::{DocumentId, Dom, DomHandle, HostHandle, ListenerOptions, NodeRef, events};
    /// use zgui_interned::ElementName;
    ///
    /// install().unwrap();
    /// let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    /// let dom = DomHandle::from_rc(backend.clone());
    /// let host = HostHandle::new(StubHost::default());
    /// let window = Mounted::new();
    ///
    /// let mine = window.with(NodeRef::new);
    /// mine.bind(dom.create_element(ElementName::new("box")), &dom, &host);
    ///
    /// let seen = Rc::new(Cell::new(0));
    /// let count = Rc::clone(&seen);
    /// let root = mine.window_root().expect("the handle is bound");
    /// let guard = root
    ///     .listen(events::POINTER_DOWN, ListenerOptions::CAPTURE, move |_| {
    ///         count.set(count.get() + 1);
    ///     })
    ///     .expect("the root is bound");
    ///
    /// assert_eq!(backend.listener_count(), 1);
    /// drop(guard);
    /// assert_eq!(backend.listener_count(), 0, "the guard took it away with it");
    /// window.unmount();
    /// ```
    #[must_use = "dropping the guard removes the listener immediately"]
    pub fn listen<E: EventType>(
        &self,
        event: E,
        options: ListenerOptions,
        handler: impl Fn(&mut EventCx<'_, E>) + 'static,
    ) -> Option<ListenerGuard> {
        let bound = self.bound()?;
        let id = bound
            .dom
            .add_listener(bound.node, event.kind(), options, erase(event, handler));
        Some(ListenerGuard::new(bound.dom, bound.node, id))
    }

    // ---- observation: geometry as a reactive input ------------------------------------------

    /// Observes this node's border box.
    ///
    /// The signal carries the value as of the last completed layout and is written during the
    /// frame that changes it, before anything is painted — so a view that repositions itself from
    /// what it observes is painted in its final place in that same frame.
    ///
    /// Observation is refcounted per node and quantity: however many callers share one, the frame
    /// pays for one, and it is released when the last of them goes. The signal handed back is
    /// yours alone and dies with your own scope. Prefer [`NodeRef::bounds`] for a one-shot read;
    /// observing is a deliberate act with a stated cost, which is why reading a getter never
    /// starts one by accident.
    pub fn observe_border_box(&self) -> Signal<Option<Rect<DevicePx, Device>>, LocalStorage> {
        self.observe_border_box_while(|| true)
    }

    /// Observes the size of this node's content area.
    pub fn observe_content_size(&self) -> Signal<Size<DevicePx, Device>, LocalStorage> {
        self.observe_content_size_while(|| true)
    }

    /// Observes this node's scroll position.
    pub fn observe_scroll(&self) -> Signal<ScrollPosition, LocalStorage> {
        self.observe_scroll_while(|| true)
    }

    /// Observes this node's border box for as long as `active` answers true.
    ///
    /// The share is given back the moment `active` turns false and taken again when it turns true,
    /// so a view that is on screen only some of the time costs the frame only while it is. Until
    /// the first value of a new watch arrives the signal reads `None`, which is the same state a
    /// freshly mounted observation is in.
    ///
    /// `active` is read reactively, so it may be any signal or any closure over signals.
    pub fn observe_border_box_while(
        &self,
        active: impl Fn() -> bool + 'static,
    ) -> Signal<Option<Rect<DevicePx, Device>>, LocalStorage> {
        let value = self.observe(Observed::BorderBox, active);
        Signal::derive_local(move || value.get().and_then(|value| value.as_border_box()))
    }

    /// Observes the size of this node's content area for as long as `active` answers true.
    ///
    /// The size reads zero while nothing is being watched, which is what an unmeasured node
    /// reports too.
    pub fn observe_content_size_while(
        &self,
        active: impl Fn() -> bool + 'static,
    ) -> Signal<Size<DevicePx, Device>, LocalStorage> {
        let value = self.observe(Observed::ContentSize, active);
        Signal::derive_local(move || {
            value
                .get()
                .and_then(|value| value.as_content_size())
                .unwrap_or_default()
        })
    }

    /// Observes this node's scroll position for as long as `active` answers true.
    pub fn observe_scroll_while(
        &self,
        active: impl Fn() -> bool + 'static,
    ) -> Signal<ScrollPosition, LocalStorage> {
        let value = self.observe(Observed::ScrollPosition, active);
        Signal::derive_local(move || {
            value
                .get()
                .and_then(|value| value.as_scroll_position())
                .unwrap_or_default()
        })
    }

    /// Takes a share in one observation, and gives it back when the calling scope goes away.
    ///
    /// The share is taken **when the handle binds**, not when this is called, and that is what
    /// makes a component able to observe its own element: a component's body runs before the view
    /// it returns is built, so at the moment it asks, the element it is asking about does not
    /// exist yet. Requiring a bound handle here would mean every such component observed nothing,
    /// for ever, with no error anywhere.
    ///
    /// `active` gates the share and is read reactively. It is read *before* the handle, so that a
    /// watch which is not wanted yet still re-runs when the element it names comes into being.
    /// A share given back also clears the value, because what a released observation last said is
    /// a measurement of a frame nobody is watching any more.
    fn observe(
        &self,
        what: Observed,
        active: impl Fn() -> bool + 'static,
    ) -> Signal<Option<crate::dom::ObservedValue>, LocalStorage> {
        let Some(registry) = crate::cx::current_observations() else {
            debug_assert!(
                false,
                "observing geometry outside a window's scope: no observation registry was provided"
            );
            return Signal::derive_local(|| None);
        };

        // What has been acquired, so that the release at the end names the same node the acquire
        // did. Held out here rather than inside the effect because the effect disposes of its own
        // scope on every run, and a release registered in there would give the share back while
        // the value it produced is still being read.
        let held: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
        let value: RwSignal<Option<ArcRwSignal<Option<crate::dom::ObservedValue>>>, LocalStorage> =
            RwSignal::new_local(None);

        let handle = *self;
        let watching = {
            let registry = registry.clone();
            let held = Rc::clone(&held);
            RenderEffect::new(move |_| {
                let wanted = active();
                // Reading the handle reactively is the whole mechanism: this re-runs when the
                // element it names comes into being.
                let Some(node) = handle.get() else {
                    return;
                };
                if !wanted {
                    if let Some(previous) = held.take() {
                        registry.release(previous, what);
                        value.try_set(None);
                    }
                    return;
                }
                if held.get() == Some(node) {
                    return;
                }
                let Some(bound) = handle.bound() else {
                    return;
                };
                if let Some(previous) = held.replace(Some(node)) {
                    registry.release(previous, what);
                }
                value.try_set(Some(registry.acquire(&bound.dom, node, what)));
            })
        };

        on_cleanup_local(move || {
            drop(watching);
            if let Some(node) = held.take() {
                registry.release(node, what);
            }
        });

        Signal::derive_local(move || {
            value
                .try_get()
                .flatten()
                .and_then(|shared| shared.try_get().flatten())
        })
    }

    // ---- focus traversal ----------------------------------------------------------------------

    /// Confines sequential focus navigation to this node's subtree until the guard is dropped.
    ///
    /// `None` when the handle is not bound, because there is no subtree to confine anything to.
    #[must_use = "dropping the guard uninstalls the trap immediately"]
    pub fn trap_focus(&self, options: FocusTrapOptions) -> Option<FocusTrap> {
        let bound = self.bound()?;
        let id = bound.host.push_focus_trap(bound.node, options);
        Some(FocusTrap::new(bound.host, bound.node, id))
    }

    /// Registers this node to hear keys that nothing in the window has focus for, until the guard
    /// is dropped.
    ///
    /// What an application-wide chord needs, and the reason it needs it: a key is delivered along
    /// the path to whatever holds focus, and a window in which nothing holds focus routes one to
    /// the document's root and no further. A listener anywhere below the root therefore hears
    /// nothing at all on a window that has just been opened and not yet touched — which is exactly
    /// when a shortcut is reached for.
    ///
    /// Only this node's own listeners are added to the route. Nothing between the root and it is,
    /// so a shortcut is not a way of making every handler in a subtree hear an unfocused key.
    ///
    /// `None` when the handle is not bound, because there is no node to register.
    #[must_use = "dropping the guard removes the registration immediately"]
    pub fn window_shortcut(&self) -> Option<WindowShortcut> {
        let bound = self.bound()?;
        bound.host.add_window_shortcut(bound.node);
        Some(WindowShortcut::new(bound.host, bound.node))
    }

    /// Focusable descendants, in sequential focus-navigation order, as of the last completed
    /// frame.
    pub fn focusables(&self) -> Vec<NodeId> {
        self.bound()
            .map(|bound| bound.host.focusables(bound.node))
            .unwrap_or_default()
    }

    /// Moves focus within this subtree, returning the node that received it.
    pub fn focus_move(&self, direction: FocusMove) -> Option<NodeId> {
        let bound = self.bound()?;
        bound.host.focus_move(bound.node, direction)
    }

    // ---- text selection ------------------------------------------------------------------------

    /// The selection in this editable node, in document offsets.
    pub fn selection(&self) -> Option<Range<usize>> {
        let bound = self.bound()?;
        bound.host.selection(bound.node)
    }

    /// Replaces the selection in this editable node.
    pub fn set_selection(&self, range: Range<usize>) {
        if let Some(bound) = self.bound() {
            bound.host.set_selection(bound.node, range);
        }
    }

    /// Selects everything in this editable node.
    pub fn select_all(&self) {
        if let Some(bound) = self.bound() {
            bound.host.select_all(bound.node);
        }
    }

    /// Puts `text` in this editable node, as the value its application owns.
    ///
    /// This is how a field is driven from a signal. Call it from an effect over the signal and the
    /// field follows it; text the field already holds does nothing at all, so the same effect
    /// running for the user's own keystroke does not move the caret they are typing at.
    ///
    /// The effect's handle has to be kept: an effect whose handle is dropped stops running, and a
    /// field bound to a dropped one follows its signal exactly once and then never again.
    ///
    /// ```no_run
    /// use zgui_reactive::{RenderEffect, RwSignal};
    /// use zgui_reactive::prelude::Get;
    /// use zgui_view::NodeRef;
    ///
    /// let field = NodeRef::new();
    /// let value = RwSignal::new_local(String::new());
    /// let binding = RenderEffect::new(move |_| field.set_value(&value.get()));
    /// zgui_reactive::on_cleanup_local(move || drop(binding));
    /// ```
    ///
    /// A node that is not editable, or that is not mounted, is left alone.
    pub fn set_value(&self, text: &str) {
        if let Some(bound) = self.bound() {
            bound.host.set_value(bound.node, text);
        }
    }

    /// How many animations and transitions are currently running on this node.
    pub fn running_animations(&self) -> usize {
        self.bound()
            .map(|bound| bound.host.running_animations(bound.node))
            .unwrap_or_default()
    }
}

impl Default for NodeRef {
    fn default() -> Self {
        Self::new()
    }
}

/// The node that currently holds focus in this window, as a reactive value.
///
/// Reads the host the enclosing window provided. Calling it outside a window's scope panics in
/// debug builds and returns a permanently empty signal in release, because a control that
/// silently never learns about focus is worse to debug than one that says so.
///
/// This is a free function rather than a method because there is no node to hang it on. Together
/// with [`NodeRef::contains`] it answers the one question every dismissable overlay and every
/// roving-focus group has to ask: is focus still inside me?
pub fn focused_node() -> Signal<Option<NodeId>, LocalStorage> {
    match current_host() {
        Some(host) => host.focused(),
        None => {
            debug_assert!(false, "focused_node() was called outside a window's scope");
            Signal::derive_local(|| None)
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_interned::ElementName;
    use zgui_reactive::Mounted;
    use zgui_reactive::prelude::*;

    use super::{NodeRef, focused_node};
    use crate::dom::ObservedValue;
    use crate::fixture::Fixture;
    use crate::host::{FocusTrapOptions, ViewHost};

    fn a_box(f: &Fixture) -> crate::NodeId {
        f.dom.create_element(ElementName::new("box"))
    }

    fn bounds(width: f32) -> ObservedValue {
        ObservedValue::BorderBox(Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(10.0)),
        ))
    }

    #[test]
    fn a_node_ref_is_copy() {
        fn requires_copy<T: Copy>() {}
        requires_copy::<NodeRef>();
    }

    #[test]
    fn an_unbound_handle_answers_rather_than_panicking() {
        let f = Fixture::new();
        let node_ref = f.window.with(NodeRef::new);
        assert_eq!(node_ref.get(), None);
        assert_eq!(node_ref.bounds(), None);
        assert_eq!(node_ref.focusables(), Vec::new());
        assert!(!node_ref.contains(a_box(&f)));
        f.window.unmount();
    }

    #[test]
    fn reading_a_handle_whose_scope_is_gone_is_not_a_panic() {
        let f = Fixture::new();
        let component = f.window.with(Mounted::new);
        let node_ref = component.with(NodeRef::new);
        node_ref.bind(a_box(&f), &f.dom, f.cx.host());
        assert!(node_ref.get().is_some());

        component.unmount();
        assert_eq!(node_ref.get(), None);
        f.window.unmount();
    }

    #[test]
    fn a_one_shot_read_reports_what_the_engine_last_measured() {
        let f = Fixture::new();
        let node = a_box(&f);
        let node_ref = f.window.with(NodeRef::new);
        node_ref.bind(node, &f.dom, f.cx.host());

        assert_eq!(node_ref.bounds(), None);
        f.engine.set_border_box(
            node,
            Rect::new(
                Point::new(DevicePx(1.0), DevicePx(2.0)),
                Size::new(DevicePx(3.0), DevicePx(4.0)),
            ),
        );
        assert_eq!(node_ref.bounds().map(|r| r.size.width), Some(DevicePx(3.0)));
        f.window.unmount();
    }

    #[test]
    fn two_observers_share_one_registration_and_neither_kills_the_other() {
        let f = Fixture::new();
        let node = a_box(&f);
        let node_ref = f.window.with(NodeRef::new);
        node_ref.bind(node, &f.dom, f.cx.host());

        let first_scope = f.window.with(Mounted::new);
        let second_scope = f.window.with(Mounted::new);
        let first = f
            .window
            .with(|| first_scope.with(|| node_ref.observe_border_box()));
        let second = f
            .window
            .with(|| second_scope.with(|| node_ref.observe_border_box()));
        assert_eq!(f.backend.observation_count(), 1);

        f.backend.deliver(node, bounds(40.0));
        assert_eq!(first.get().map(|r| r.size.width), Some(DevicePx(40.0)));
        assert_eq!(second.get().map(|r| r.size.width), Some(DevicePx(40.0)));

        // The first observer goes away. The second one keeps working, and the registration lives.
        first_scope.unmount();
        assert_eq!(f.backend.observation_count(), 1);
        f.backend.deliver(node, bounds(50.0));
        assert_eq!(second.get().map(|r| r.size.width), Some(DevicePx(50.0)));

        second_scope.unmount();
        assert_eq!(f.backend.observation_count(), 0);
        f.window.unmount();
    }

    #[test]
    fn observing_a_handle_that_is_not_bound_yet_starts_when_it_binds() {
        // The case every component that measures its own element is in: the body runs before the
        // view it returns is built, so at the moment it asks, the element does not exist. An
        // observation that required a bound handle would leave it observing nothing for ever, and
        // nothing anywhere would say so.
        let f = Fixture::new();
        let node_ref = f.window.with(NodeRef::new);
        let observed = f.window.with(|| node_ref.observe_border_box());
        assert_eq!(observed.get(), None);
        assert_eq!(f.backend.observation_count(), 0, "nothing to observe yet");

        let node = a_box(&f);
        node_ref.bind(node, &f.dom, f.cx.host());
        zgui_reactive::flush();
        assert_eq!(f.backend.observation_count(), 1, "the binding started it");

        f.backend.deliver(node, bounds(64.0));
        assert_eq!(observed.get().map(|r| r.size.width), Some(DevicePx(64.0)));

        f.window.unmount();
    }

    #[test]
    fn an_observation_started_by_a_binding_is_released_with_its_scope() {
        let f = Fixture::new();
        let node_ref = f.window.with(NodeRef::new);
        let scope = f.window.with(Mounted::new);
        let _observed = f
            .window
            .with(|| scope.with(|| node_ref.observe_content_size()));

        node_ref.bind(a_box(&f), &f.dom, f.cx.host());
        zgui_reactive::flush();
        assert_eq!(f.backend.observation_count(), 1);

        scope.unmount();
        assert_eq!(f.backend.observation_count(), 0);
        f.window.unmount();
    }

    #[test]
    fn a_scroll_observation_delivers_the_offset_a_virtualised_list_reads() {
        use crate::scroll::ScrollPosition;
        use zgui_geom::{Point, Size};

        let f = Fixture::new();
        let node = a_box(&f);
        let node_ref = f.window.with(NodeRef::new);
        node_ref.bind(node, &f.dom, f.cx.host());

        let offsets = f.window.with(|| node_ref.observe_scroll());
        assert_eq!(offsets.get().offset.y, DevicePx(0.0));

        f.backend.deliver(
            node,
            ObservedValue::ScrollPosition(ScrollPosition {
                offset: Point::new(DevicePx(0.0), DevicePx(240.0)),
                content_size: Size::new(DevicePx(400.0), DevicePx(10_000.0)),
                scrollport: Size::new(DevicePx(400.0), DevicePx(600.0)),
            }),
        );
        assert_eq!(offsets.get().offset.y, DevicePx(240.0));
        assert!(!offsets.get().is_at_end_vertically());
        f.window.unmount();
    }

    #[test]
    fn a_content_size_observation_starts_empty_and_follows_what_layout_reports() {
        use zgui_geom::Size;

        let f = Fixture::new();
        let node = a_box(&f);
        let node_ref = f.window.with(NodeRef::new);
        node_ref.bind(node, &f.dom, f.cx.host());

        let size = f.window.with(|| node_ref.observe_content_size());
        assert_eq!(size.get(), Size::default());

        f.backend.deliver(
            node,
            ObservedValue::ContentSize(Size::new(DevicePx(120.0), DevicePx(30.0))),
        );
        assert_eq!(size.get().width, DevicePx(120.0));
        f.window.unmount();
    }

    #[test]
    fn a_focus_trap_is_uninstalled_when_its_guard_goes() {
        let f = Fixture::new();
        let node_ref = f.window.with(NodeRef::new);
        node_ref.bind(a_box(&f), &f.dom, f.cx.host());

        let guard = node_ref
            .trap_focus(FocusTrapOptions::MODAL)
            .expect("the handle is bound");
        assert_eq!(f.engine.live_focus_traps(), 1);
        drop(guard);
        assert_eq!(f.engine.live_focus_traps(), 0);
        f.window.unmount();
    }

    #[test]
    fn the_focused_node_is_the_same_signal_for_every_caller_in_a_window() {
        let f = Fixture::new();
        let node = a_box(&f);

        let scope = f.window.with(Mounted::new);
        let first = scope.with(focused_node);
        let second = f.window.with(focused_node);

        f.engine.focus(node);
        assert_eq!(first.get(), Some(node));
        assert_eq!(second.get(), Some(node));

        // The first reader's scope goes away; the second one still reads.
        scope.unmount();
        assert_eq!(second.get(), Some(node));
        f.window.unmount();
    }
}
