//! What the desktop knows about its outputs.

use winit::monitor::MonitorHandle;
use zgui_geom::{DevicePx, Point, Size};
use zgui_platform::MonitorInfo;

/// What is known about one output.
///
/// The refresh rate is carried in thousandths of a hertz exactly as the platform reports it,
/// because the rates that matter are not whole numbers: a display sold as sixty hertz usually runs
/// at 59.94, and a deadline computed against sixty either misses a frame or presents twice.
///
/// The scale factor here is the *output's*, and it is deliberately not where a window's scale comes
/// from. A window can be presented at a scale that is not the scale of the output it happens to
/// overlap, and reading the output's instead produces a subtly wrong size on exactly the
/// arrangements that are hardest to reproduce.
pub(crate) fn describe(monitor: &MonitorHandle) -> MonitorInfo {
    let position = monitor.position();
    let size = monitor.size();
    let mut info = MonitorInfo::new(
        Point::new(DevicePx(position.x as f32), DevicePx(position.y as f32)),
        Size::new(DevicePx(size.width as f32), DevicePx(size.height as f32)),
        monitor.scale_factor(),
    );
    if let Some(name) = monitor.name() {
        info = info.with_name(name);
    }
    // A rate of zero means the platform declined to answer, and inventing sixty here would hide
    // that from the fallback that is stated once, in the contract, and applied everywhere.
    if let Some(rate) = monitor.refresh_rate_millihertz().filter(|rate| *rate > 0) {
        info = info.with_refresh_rate_millihertz(rate);
    }
    info
}
