//! A window to build a component into, and the frames that drive it.

use core::time::Duration;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_reactive::Mounted;
use zgui_view::{BuildCxOwned, DocumentId, DomHandle, HostHandle, NodeId};

use crate::dom::RecordingDom;
use crate::host::ScriptedHost;
use crate::input::Dispatcher;
use crate::transcript::Transcript;

/// Everything a component test needs, wired together.
///
/// One transcript, one tree, one host, one reactive scope and one root element. Building a
/// component into this and then driving it is the whole of a component test.
///
/// ```
/// use zgui_interned::ElementName;
/// use zgui_testkit_view::Window;
/// use zgui_view::Dom;
///
/// let window = Window::open();
/// let button = window.dom.create_element(ElementName::new("control"));
/// window.dom.insert(window.root, button, None);
/// window.place(button, 0.0, 0.0, 80.0, 24.0);
///
/// // A frame is a flush of everything the last interaction set going.
/// window.frame();
/// assert_eq!(window.dom.parent(button), Some(window.root));
/// ```
pub struct Window {
    /// What everything records into.
    pub transcript: Transcript,
    /// The tree.
    pub dom: Rc<RecordingDom>,
    /// The host.
    pub host: Rc<ScriptedHost>,
    /// The handle a view holds on the tree.
    pub dom_handle: DomHandle,
    /// The handle a view holds on the host.
    pub host_handle: HostHandle,
    /// The scope everything built here belongs to.
    pub scope: Mounted,
    /// The context views are built through.
    pub cx: BuildCxOwned,
    /// The window's root element.
    pub root: NodeId,
}

impl Window {
    /// Opens one.
    ///
    /// # Panics
    ///
    /// Panics if the reactive runtime cannot be installed on this thread.
    pub fn open() -> Self {
        Self::for_document(DocumentId::FIRST)
    }

    /// Opens one belonging to a particular document, for a test that needs two.
    ///
    /// # Panics
    ///
    /// Panics if the reactive runtime cannot be installed on this thread.
    pub fn for_document(document: DocumentId) -> Self {
        zgui_reactive::install().ok();
        let transcript = Transcript::new();
        let dom = Rc::new(RecordingDom::with_transcript(document, transcript.clone()));
        let host = Rc::new(ScriptedHost::with_transcript(transcript.clone()));
        let dom_handle = DomHandle::from_rc(Rc::clone(&dom) as Rc<dyn zgui_view::Dom>);
        let host_handle = HostHandle::from_rc(Rc::clone(&host) as Rc<dyn zgui_view::ViewHost>);
        let scope = Mounted::new();
        // Inside the window's own scope, exactly as a running window does it: the free functions a
        // component reaches for — a timeout, an interval, whatever holds focus — resolve the host
        // through the scope tree rather than through a global, and a harness that skipped this
        // would make every one of them panic in a component that is perfectly correct.
        scope.with(|| zgui_view::provide_host(host_handle.clone()));
        let cx = BuildCxOwned::new(
            dom_handle.clone(),
            host_handle.clone(),
            scope.owner().clone(),
            document,
        );
        let root = {
            use zgui_view::Dom;
            let root = dom.create_element(zgui_interned::ElementName::new("root"));
            // The window's root and the root a view reaches through `NodeRef::window_root` are the
            // same element, exactly as they are in a running window. Left undeclared they would be
            // two, and every outside-press test would pass while listening on nothing.
            dom.set_root(root);
            root
        };
        Self {
            transcript,
            dom,
            host,
            dom_handle,
            host_handle,
            scope,
            cx,
            root,
        }
    }

    /// Declares where a node's box is, which is what a scripted press is aimed with.
    pub fn place(&self, node: NodeId, x: f32, y: f32, width: f32, height: f32) {
        self.host.set_border_box(
            node,
            Rect::new(
                Point::new(DevicePx(x), DevicePx(y)),
                Size::new(DevicePx(width), DevicePx(height)),
            ),
        );
    }

    /// Something to aim events with.
    pub fn dispatcher(&self) -> Dispatcher<'_> {
        Dispatcher::new(&self.dom, &self.host, self.root)
    }

    /// Clicks at a point, and reports what that reached.
    pub fn click(&self, x: f32, y: f32) -> crate::input::Delivered {
        self.dispatcher()
            .click_at(Point::new(DevicePx(x), DevicePx(y)))
    }

    /// Runs everything a frame runs, which at this layer is the reactive flush.
    ///
    /// A signal written by a handler does not reach the tree until effects run, so a test that
    /// asserted on the tree straight after an interaction would be asserting on the frame before
    /// the one it caused. This is that frame — and it is a call rather than something that happens
    /// on its own, so the two are never confused.
    pub fn frame(&self) {
        // Bounded, exactly as a window's own loop is: a value written into a field can change what
        // the field says, which can run an effect that writes another, and a harness that settled
        // only once would show the frame before the one the interaction caused.
        for _ in 0..8u8 {
            zgui_reactive::flush();
            let written = self.host.take_written_values();
            if written.is_empty() {
                return;
            }
            for (node, text) in written {
                self.dom.load_value(node, &text);
            }
        }
    }

    /// Moves the clock, firing everything that comes due, and then runs a frame.
    ///
    /// The pair, because a timer callback that writes a signal has produced work no frame has run
    /// yet — which is precisely the delay a tooltip is made of.
    pub fn advance(&self, by: Duration) {
        self.host.advance(by);
        self.frame();
    }

    /// The virtual clock's reading.
    pub fn now(&self) -> Duration {
        self.host.now()
    }

    /// Where a node's box is, as the test declared it.
    pub fn bounds_of(&self, node: NodeId) -> Option<Rect<DevicePx, Device>> {
        use zgui_view::ViewHost;
        self.host.border_box(node)
    }
}

impl core::fmt::Debug for Window {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Window")
            .field("root", &self.root)
            .field("recorded", &self.transcript.len())
            .finish_non_exhaustive()
    }
}
