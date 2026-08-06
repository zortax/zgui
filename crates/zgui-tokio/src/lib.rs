//! tokio, for a zgui application.
//!
//! zgui runs without this crate. Its reactive layer has its own single-threaded executor on the UI
//! thread and a small worker pool behind [`background`](zgui_reactive::background), and nothing in
//! the framework asks for a runtime. What it cannot do on its own is run the parts of tokio that
//! need a *runtime context* rather than merely a thread: `tokio::time`, `tokio::net`, `tokio::fs`,
//! and every library built on them — `reqwest`, `sqlx`, `tonic`. Those panic when constructed
//! outside a runtime, however correctly they are afterwards polled.
//!
//! Installing this crate closes that gap in one call:
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let _tokio = zgui_tokio::install()?;
//!     // ... run the application ...
//!     Ok(())
//! }
//! ```
//!
//! # What installing does
//!
//! **Background work runs on the runtime.** [`background`](zgui_reactive::background) and
//! [`blocking`](zgui_reactive::blocking) are routed to it, so a future handed to `background` is
//! polled inside a runtime context and a tokio-based library works there with no further ceremony.
//! `blocking` reaches tokio's own blocking pool, which is separate from its worker threads — the
//! thing the default executor does not have.
//!
//! **The UI thread's own pool is polled inside the runtime too.** That is the part that makes
//! tokio feel native rather than bolted on: a task spawned with
//! [`spawn`](zgui_reactive::spawn) may await a tokio timer or a socket *directly*, on the UI
//! thread, and the wake arriving from the reactor's thread reaches the frame loop through the
//! ordinary wake edge.
//!
//! ```no_run
//! # use zgui_reactive::{RwSignal, spawn};
//! # use zgui_reactive::prelude::*;
//! # fn example(visible: RwSignal<bool>) {
//! spawn(async move {
//!     tokio::time::sleep(std::time::Duration::from_secs(3)).await;
//!     visible.set(false); // a toast that dismisses itself
//! });
//! # }
//! ```
//!
//! The cost of that convenience is that awaiting something slow on the UI thread is now easy to do
//! by accident, and it holds the frame exactly as any other slow poll would. The rule does not
//! change: *waiting* on the UI thread is free, *working* on it is not. A request whose response
//! must be parsed belongs in `background`; a request that merely has to be waited for does not.
//!
//! # What does not need this crate
//!
//! `tokio::sync` — `mpsc`, `watch`, `broadcast`, `oneshot`, `Mutex`, `Semaphore` — is
//! runtime-agnostic. Those channels can be awaited on zgui's UI-thread pool with no runtime at
//! all, and [`spawn_stream`](zgui_reactive::spawn_stream) drives them into signals. The helpers in
//! [`bridge`] are conveniences over that, not a bridge across anything.
//!
//! # One executor slot
//!
//! `any_spawner`, which the reactive engine spawns through, has a single process-wide slot, and
//! zgui claims it with its own UI-thread executor at startup. This crate never touches that slot:
//! it fills zgui's *background* seam instead. Installing tokio and installing zgui therefore do
//! not compete, in either order — asserted by a test, because the failure would otherwise be
//! `InstallError::ForeignExecutor` from somewhere that never mentions tokio.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod bridge;
mod context;
mod spawner;

use std::sync::Arc;

use tokio::runtime::{Builder, Handle, Runtime};

pub use bridge::{spawn_broadcast, spawn_receiver, spawn_watch, watch_signal};

/// Why a tokio runtime could not be made zgui's background executor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The runtime could not be built.
    #[error("could not build a tokio runtime: {0}")]
    Build(#[from] std::io::Error),

    /// Background work had already been given to another executor.
    ///
    /// Install before the first [`background`](zgui_reactive::background) or
    /// [`blocking`](zgui_reactive::blocking) call — in practice, at the top of `main`, before the
    /// application runs. Installing later would leave work already in flight on the default pool
    /// while everything after it ran on tokio.
    #[error(transparent)]
    Spawner(#[from] zgui_reactive::SpawnerError),
}

/// A runtime installed as zgui's background executor, shut down when this is dropped.
///
/// Hold it for as long as the application runs — `let _tokio = install()?;` at the top of `main`
/// is the whole pattern. Dropping it shuts the runtime down, at which point any background work
/// still in flight is abandoned and whatever was awaiting it panics rather than hanging.
#[must_use = "dropping the guard shuts the runtime down, and with it every background task"]
#[derive(Debug)]
pub struct Installed {
    /// The runtime, unless this guard was made from a handle somebody else owns.
    runtime: Option<Runtime>,
}

impl Installed {
    /// A handle to the installed runtime, for code that needs to name one.
    #[must_use]
    pub fn handle(&self) -> Handle {
        match &self.runtime {
            Some(runtime) => runtime.handle().clone(),
            None => Handle::current(),
        }
    }
}

/// Builds a multi-threaded runtime and makes it zgui's background executor.
///
/// Call once, from the UI thread, before the application runs.
///
/// # Errors
///
/// [`Error::Build`] if the runtime could not be started, or [`Error::Spawner`] if background work
/// has already gone to a different executor.
pub fn install() -> Result<Installed, Error> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_name("zgui-tokio")
        .build()?;
    adopt(runtime.handle().clone())?;
    Ok(Installed {
        runtime: Some(runtime),
    })
}

/// Makes an existing runtime zgui's background executor.
///
/// For an application that already has a runtime — one built by `#[tokio::main]`, or one shared
/// with a server half. Nothing is shut down when the returned guard is dropped, because nothing
/// here owns the runtime.
///
/// Call from the UI thread, before the application runs.
///
/// # Errors
///
/// [`Error::Spawner`] if background work has already gone to a different executor.
pub fn install_handle(handle: Handle) -> Result<Installed, Error> {
    adopt(handle)?;
    Ok(Installed { runtime: None })
}

/// Routes background work to `handle`, and polls the UI thread's pool inside it.
fn adopt(handle: Handle) -> Result<(), zgui_reactive::SpawnerError> {
    zgui_reactive::set_background_spawner(Arc::new(spawner::TokioSpawner::new(handle.clone())))?;
    enter_here(handle);
    tracing::debug!("tokio is zgui's background executor, and the frame flush enters it");
    Ok(())
}

/// Polls *this* thread's reactive task pool inside `handle`, without touching anything else.
///
/// [`install`] does this for the thread it is called on, which is all a single-window application
/// needs. The two things it installs have different scopes, though: the background executor is
/// one per process, while entering the runtime is a property of one thread's flush. So a program
/// with a second UI thread — a second window driven independently, or a test harness running a
/// complete runtime per test — calls this on that thread to give it the same reach, rather than
/// installing a second runtime it does not need.
///
/// Call from a thread that has installed a reactive runtime, before its first frame.
pub fn enter_here(handle: Handle) {
    zgui_reactive::set_poll_context(std::rc::Rc::new(context::Entered::new(handle)));
}

impl Drop for Installed {
    fn drop(&mut self) {
        // `shutdown_background` rather than the default blocking drop: the UI thread is the one
        // dropping this, usually while the window is closing, and a runtime with a task that will
        // not finish would otherwise hang the program on the way out instead of ending it.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
