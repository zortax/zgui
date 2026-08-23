//! One output, as the contract describes it.

use std::time::Duration;

use smithay_client_toolkit::output::OutputInfo;
use zgui_geom::{DevicePx, Point, Size};
use zgui_platform::MonitorInfo;

/// What the contract knows about one output.
///
/// The extent is the mode the output is running, in its own pixels. The position is where the
/// compositor placed it in the desktop's coordinates, which the protocol reports in logical units;
/// it is scaled here so that every field of the answer is in the same space.
pub fn describe(info: &OutputInfo) -> MonitorInfo {
    let (width, height) = info
        .modes
        .iter()
        .find(|mode| mode.current)
        .map_or((0, 0), |mode| mode.dimensions);
    let scale = f64::from(info.scale_factor.max(1));
    let (x, y) = info.logical_position.unwrap_or((0, 0));
    let monitor = MonitorInfo::new(
        Point::new(
            DevicePx((f64::from(x) * scale) as f32),
            DevicePx((f64::from(y) * scale) as f32),
        ),
        Size::new(DevicePx(width as f32), DevicePx(height as f32)),
        scale,
    );
    let monitor = match &info.name {
        Some(name) => monitor.with_name(name.clone()),
        None => monitor,
    };
    match refresh_rate(info) {
        Some(rate) => monitor.with_refresh_rate_millihertz(rate),
        None => monitor,
    }
}

/// The rate the output's current mode runs at, in thousandths of a hertz.
///
/// The protocol reports exactly that unit, so nothing is converted. A rate of zero means the
/// output did not say, and is reported as unknown rather than as a stopped display.
pub fn refresh_rate(info: &OutputInfo) -> Option<u32> {
    info.modes
        .iter()
        .find(|mode| mode.current)
        .map(|mode| mode.refresh_rate)
        .filter(|rate| *rate > 0)
        .map(|rate| rate as u32)
}

/// The interval between refreshes of the output's current mode.
pub fn interval(info: &OutputInfo) -> Option<Duration> {
    let rate = refresh_rate(info)?;
    Some(Duration::from_secs_f64(1_000.0 / f64::from(rate)))
}

/// The scale of the sharpest output in `infos`, for a surface that is on all of them.
///
/// The largest rather than the first: a window straddling a 1x and a 2x monitor drawn at 1x is
/// visibly soft on half of itself, and drawn at 2x is merely oversampled on the other half.
pub fn sharpest<'a>(infos: impl Iterator<Item = &'a OutputInfo>) -> Option<i32> {
    infos.map(|info| info.scale_factor.max(1)).max()
}
