//! Opening a window, and running one phase over every document size.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::runtime::Runtime;
use zgui_platform_headless::Harness;

use crate::gallery::{HEIGHT, SIZES, WIDTH, runtime};

/// Runs one phase at every document size, smallest first, and stops at the first size that fails.
///
/// The smallest sizes are where a comparison is likeliest to be vacuous — the shell alone draws a
/// handful of primitives, and a step that draws nothing there draws something at every larger size
/// — so a differential that is only ever run on the shipped document cannot see the class of defect
/// that only shows on a nearly empty page. Each size is a process of its own because a document is
/// mounted through thread-local state that a second mount in one process would add to rather than
/// replace.
///
/// # Panics
///
/// Panics when a size fails, naming it, so a sweep cannot be read as a pass because its last line
/// said so.
pub(crate) fn every_size(phase: &str, repeats: usize) {
    let binary = std::env::current_exe().expect("the running binary can be named");
    for size in SIZES {
        println!("== {phase} {size}");
        let status = std::process::Command::new(&binary)
            .args([phase, size, &repeats.to_string()])
            .status()
            .expect("the sweep can run this binary again");
        assert!(status.success(), "{phase} failed at size {size}: {status}");
    }
    println!("SWEEP phase={phase} sizes={}", SIZES.len());
}

/// Opens a window over `runtime`, sized as every measurement here sizes it.
///
/// The resize is what gives the surface an extent at all: a window that was never told how big it
/// is lays out against nothing and every box in it is zero wide.
pub(crate) fn harness(runtime: Runtime) -> Harness<Runtime> {
    let mut harness = Harness::new(runtime);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(256);
    harness
}

/// The same, for one of the gallery's document sizes.
pub(crate) fn opened(size: &str) -> Harness<Runtime> {
    harness(runtime(size))
}
