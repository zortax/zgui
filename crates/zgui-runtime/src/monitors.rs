//! The outputs the desktop has, as something a view can be built from.
//!
//! ```no_run
//! use zgui_runtime::monitors::use_monitors;
//! use zgui_reactive::prelude::*;
//!
//! # fn example() {
//! // Every connected output. The signal changes when an output is connected, disconnected or
//! // changed, so a shell that keeps one bar per output reads it in an effect.
//! let monitors = use_monitors();
//! let names: Vec<_> = monitors
//!     .get_untracked()
//!     .iter()
//!     .filter_map(|monitor| monitor.name.clone())
//!     .collect();
//! # let _ = names;
//! # }
//! ```

use zgui_platform::{MonitorInfo, PlatformCx};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};

/// Every output the desktop has, kept up to date by the runtime.
///
/// Provided above every window, so it is the same list in all of them and in the application's
/// own scope.
#[derive(Clone)]
pub(crate) struct Monitors {
    /// The outputs, as the platform last reported them.
    list: ArcRwSignal<Vec<MonitorInfo>>,
}

impl Monitors {
    /// An empty list, filled when the platform first reports its outputs.
    pub(crate) fn new() -> Self {
        Self {
            list: ArcRwSignal::new(Vec::new()),
        }
    }

    /// Reads the outputs from `cx` again, changes the signal only when they changed, and
    /// reports whether they did.
    pub(crate) fn refresh(&self, cx: &dyn PlatformCx) -> bool {
        let monitors = cx.monitors();
        let changed = self.list.with_untracked(|held| *held != monitors);
        if changed {
            self.list.set(monitors);
        }
        changed
    }

    /// The outputs, as a signal.
    pub(crate) fn watch(&self) -> Signal<Vec<MonitorInfo>> {
        self.list.clone().into()
    }
}

/// Every output the desktop has.
///
/// Works inside any window and in the application's own scope. The list is empty until the
/// platform has reported its outputs, and stays empty on a platform with none to report.
///
/// # Panics
///
/// Panics when called outside a running application. Use [`try_use_monitors`] where code can
/// run outside one.
pub fn use_monitors() -> Signal<Vec<MonitorInfo>> {
    try_use_monitors().expect("this code is not running inside an application")
}

/// Every output the desktop has, if an application is running.
pub fn try_use_monitors() -> Option<Signal<Vec<MonitorInfo>>> {
    zgui_reactive::use_local_context::<Monitors>().map(|monitors| monitors.watch())
}
