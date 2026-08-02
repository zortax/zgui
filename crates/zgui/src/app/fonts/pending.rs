//! A font collection that is being built somewhere else.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use zgui_text_parley::{FontSystem, FontSystemOptions};

/// How many collections over this machine's installed faces this process has built.
static SYSTEM_COLLECTIONS: AtomicU64 = AtomicU64::new(0);

/// How many times this process has enumerated the faces installed on this machine.
///
/// Building that index means reading every font configuration file on the machine and walking
/// every face it names, which on a desktop with a few thousand of them is the largest single thing
/// an application does before it has a window. An application shipping its own faces asks for none
/// of it, and this is how that is stated as a number rather than as an intention.
pub fn system_collections_built() -> u64 {
    SYSTEM_COLLECTIONS.load(Ordering::Relaxed)
}

/// A collection, or a worker that is building one.
///
/// Enumerating installed faces neither reads nor writes anything the rest of a launch touches, so
/// there is no reason for it to happen *before* the window, the graphics instance and the device —
/// only a reason for it to have happened before the first cascade, which is a long way further on.
/// So it is started on a thread of its own and collected at first use, and on any machine where
/// opening a device costs more than enumerating fonts it costs the launch nothing at all.
#[derive(Debug)]
pub(super) struct Pending {
    /// The collection, once somebody has waited for it.
    ready: OnceLock<Arc<FontSystem>>,
    /// The worker building it, until somebody has.
    worker: Mutex<Option<JoinHandle<FontSystem>>>,
}

impl Pending {
    /// A collection that is already built.
    pub(super) fn ready(system: FontSystem) -> Self {
        let ready = OnceLock::new();
        let _ = ready.set(Arc::new(system));
        Self {
            ready,
            worker: Mutex::new(None),
        }
    }

    /// Starts building this machine's collection, and returns before it is built.
    pub(super) fn system() -> Self {
        // A machine that will not start a thread still has to be able to draw text, so the work
        // happens here instead and the only thing lost is the overlap.
        let worker = std::thread::Builder::new()
            .name("zgui-fonts".to_owned())
            .spawn(build)
            .ok();
        match worker {
            Some(worker) => Self {
                ready: OnceLock::new(),
                worker: Mutex::new(Some(worker)),
            },
            None => Self::ready(build()),
        }
    }

    /// The collection, waiting for the worker if it has not been waited for already.
    ///
    /// # Panics
    ///
    /// Panics with whatever the worker panicked with, on the thread that asked for the faces
    /// rather than on one nobody is watching.
    pub(super) fn get(&self) -> &Arc<FontSystem> {
        self.ready.get_or_init(|| {
            let worker = self
                .worker
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let system = match worker {
                Some(worker) => worker.join().unwrap_or_else(|panic| {
                    std::panic::resume_unwind(panic);
                }),
                // A collection with no worker behind it was handed one at construction, so this
                // branch is unreachable — and building one is a better answer to being wrong
                // about that than a process that stops because it cannot draw text.
                None => build(),
            };
            Arc::new(system)
        })
    }
}

/// Builds one collection over this machine's installed faces, and says that it did.
fn build() -> FontSystem {
    SYSTEM_COLLECTIONS.fetch_add(1, Ordering::Relaxed);
    FontSystem::new(FontSystemOptions::with_system_fonts())
}
