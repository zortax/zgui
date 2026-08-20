//! What a component author names.
//!
//! Everything here is re-exported from the crate root as well; this module exists so that an
//! application can bring the whole working set into scope at once.
//!
//! ```
//! use zgui_view::prelude::*;
//! ```

pub use zgui_reactive::prelude::*;
// `LocalStorage` is here because a signal holding anything from this layer needs it — a node
// handle, a callback, a backend handle — and a name a view cannot be written without does not
// belong two imports away from the constructors that take it.
pub use zgui_reactive::{
    Callable, Callback, LocalStorage, Memo, Owner, ReadSignal, RwSignal, Signal, StoredValue,
    Trigger, UnsyncCallback, WriteSignal, expect_context, on_cleanup_local, provide_context,
    provide_local_context, signal, signal_local, take_context, update_context, use_context,
    use_local_context, with_context,
};
// Doing something that takes time, which is not something a view can be written without either.
// `spawn` runs on the UI thread and may touch anything reactive; `background` and `blocking` are
// how a caller gets off it and back again; `ui` is how anything else gets on to it.
pub use zgui_reactive::{Task, background, blocking, spawn, spawn_local, ui};

// `Binding` itself is deliberately absent: it is what an element implementation builds to keep
// one property in step with one value, and it is reachable at `zgui::view::Binding` by the few
// authors who write one. Leaving it here would put two different meanings of the word into every
// application's namespace at once.
pub use crate::binding::{A11yBinding, Attrs, Classes};
pub use crate::cx::{BuildCx, BuildCxOwned};
pub use crate::dom::{Dom, DomHandle, Observed, OverlayLayer};
pub use crate::event::{EventCx, EventType, events, handler};
pub use crate::flow::{
    Await, Dynamic, ErrorBoundary, For, ForProps, Portal, PortalProps, Show, ShowProps, Suspense,
    SuspenseContext, Transition, ViewError,
};
pub use crate::host::{
    FocusMove, FocusTrap, FocusTrapOptions, HostHandle, ViewHost, WindowShortcut,
};
pub use crate::id::{DocumentId, NodeId};
pub use crate::node_ref::{ListenerGuard, NodeRef, focused_node};
pub use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};
pub use crate::sheet::{Stylesheet, install_stylesheet, remove_stylesheet};
pub use crate::time::{Timers, request_frame, set_interval, set_timeout};
pub use crate::value::{IntoReactiveValue, ReactiveValue};
pub use crate::view::{Anchor, AnyView, Children, ChildrenFn, Either, IntoView, View};
pub use zgui_vocab::{ListenerOptions, Role};
