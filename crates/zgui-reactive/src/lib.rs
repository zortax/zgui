//! Signals, effects and the single-threaded executor the frame loop drives.
//!
//! This crate is the framework's whole reactive surface. Everything a view, a component or an
//! application touches — signals, memos, stores, contexts, ownership and cancellation — is
//! published here, and the underlying reactive engine is named nowhere else in the workspace.
//! That is deliberate: the engine's version, its feature set and the handful of its APIs that
//! are unsafe to use in a UI are decided once, here.
//!
//! # The three rules
//!
//! **One thread runs reactivity.** [`install`] claims the calling thread as the UI thread and
//! installs a task pool that lives on it. Signals may be *read and written* from any thread;
//! reactive tasks only ever *run* on the UI thread. [`assert_ui_thread`] is the debug guard.
//! Work that is not reactive — a parse, a request, a decode — belongs elsewhere, and [`background`]
//! is how it gets there and comes back; [`ui`] is how a thread that was never asked about any of
//! this gets a closure onto the UI thread. See [`task`].
//!
//! **Nothing exists outside an owner.** Contexts, stored values and every arena-backed handle
//! are attached to the [`Owner`] that was current when they were created. With no current owner
//! they are silently discarded and permanently leaked — no panic, no log. [`assert_owner`] is
//! the debug guard, and [`Mounted`] is the protocol that keeps one owner per mounted node and
//! disposes of it *synchronously* on unmount.
//!
//! **Work only happens at [`flush`].** Writing a signal marks its observers and wakes their
//! tasks; it does not run them. The frame loop calls [`flush`] once per frame, which polls every
//! ready task to a stall under a bounded iteration budget and reports whether another frame is
//! owed. A wake that arrives from anywhere else — a worker thread, a completed download, a timer
//! — is routed to the [`FrameWaker`] so the event loop schedules that frame.
//!
//! # A minimal cycle
//!
//! ```
//! use zgui_reactive::prelude::*;
//! use zgui_reactive::{Mounted, RenderEffect, RwSignal, flush, install};
//!
//! install().expect("no other executor is installed");
//! let root = Mounted::new();
//!
//! let (count, doubled) = root.with(|| {
//!     let count = RwSignal::new(1);
//!     let doubled = RwSignal::new(0);
//!     let effect = RenderEffect::new(move |_| doubled.set(count.get() * 2));
//!     std::mem::forget(effect); // a real caller stores this in its view state
//!     (count, doubled)
//! });
//!
//! assert_eq!(doubled.get(), 2); // a render effect's first run is synchronous
//!
//! count.set(21);
//! assert_eq!(doubled.get(), 2); // ... and every later run waits for the flush
//!
//! flush();
//! assert_eq!(doubled.get(), 42);
//!
//! root.unmount();
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`executor`] | [`install`], [`flush`], the wake edge, the iteration budget, the debug guards |
//! | [`task`] | [`spawn`], [`background`], [`blocking`], [`ui`] and what cancels each |
//! | [`own`] | [`Mounted`], [`Scope`] and [`on_cleanup_local`] |
//! | [`context`] | [`provide_context`] and the `!Send` variants |
//! | [`store`] | Keyed stores for schema-shaped state |
//! | [`zone`] | Suppression of the "read outside a tracking context" diagnostic |
//! | [`reexport`] | Signals, memos, callbacks, async values and the evicting [`Selector`] |
//! | [`canary`] | [`effects_are_enabled`], the startup check for the silent-failure mode |
//!
//! # What is deliberately absent
//!
//! Four items of the underlying engine are not published, because each one fails silently rather
//! than loudly:
//!
//! * unkeyed store indexing — it re-runs every sibling's observers and panics on a stale index
//!   when the collection shrinks. Key your collections and address them by key instead.
//! * `Effect`, whose thread-safe constructors run effects off the UI thread. Use
//!   [`RenderEffect`], whose first run is synchronous and whose lifetime is its handle's.
//! * the engine's own `on_cleanup`, which requires a `Send + Sync` closure and therefore cannot
//!   capture a node handle. [`on_cleanup_local`] is the replacement.
//!
//! # What a task costs
//!
//! A task is cancelled when the scope that spawned it is disposed of, and the handle [`spawn`]
//! returns is for cancelling early rather than for keeping it alive — dropping it does not cancel
//! anything. That is the one place this crate departs from the framework's usual "the handle is
//! the lifetime" shape, and [`task`] says why.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod canary;
pub mod context;
pub mod executor;
pub mod own;
pub mod prelude;
pub mod reexport;
pub mod store;
pub mod task;
pub mod zone;

pub use canary::effects_are_enabled;
pub use context::{
    expect_context, provide_context, provide_local_context, take_context, update_context,
    use_context, use_local_context, with_context,
};
pub use executor::{
    FlushOutcome, FrameWaker, InstallError, PollContext, TestWaker, assert_owner, assert_ui_thread,
    flush, install, is_ui_thread, set_frame_waker, set_poll_context,
};
pub use own::{Mounted, Owner, Scope, StoredValue, on_cleanup_local};
pub use reexport::{
    Action, ArcAction, ArcAsyncDerived, ArcMemo, ArcReadSignal, ArcRwSignal, ArcSignal, ArcTrigger,
    ArcWriteSignal, AsyncDerived, AsyncTransition, Callable, Callback, LocalStorage, MaybeProp,
    Memo, ReadSignal, RenderEffect, RwSignal, Selector, Signal, SignalSetter, Storage, SyncStorage,
    Trigger, UnsyncCallback, WriteSignal, arc_signal, create_slice, signal, signal_local,
};
pub use store::{ArcField, AtKeyed, Field, KeyedSubfield, Patch, Store, StoreField, Subfield};
pub use task::{
    Background, BackgroundSpawner, SpawnerError, Task, Ui, background, blocking, provide_task_set,
    set_background_spawner, signal_from_stream, spawn, spawn_detached, spawn_local, spawn_stream,
    ui,
};
pub use zone::{NonReactiveZone, enter_non_reactive_zone};
