//! One output, as the contract describes it.

use std::time::Duration;

use smithay_client_toolkit::output::OutputInfo;
use smithay_client_toolkit::reexports::client::protocol::wl_output::{Subpixel, Transform};
use zgui_geom::{CssPx, DevicePx, Point, Rect, Size};
use zgui_platform::{MonitorId, MonitorInfo, MonitorTransform, SubpixelLayout};

/// What the contract knows about one output.
///
/// The extent is the mode the output is running, in its own pixels. The position is where the
/// compositor placed it in the desktop's coordinates, which the protocol reports in logical units;
/// it is scaled here so that every field of the answer is in the same space. The logical place
/// and extent are given as the compositor reports them.
///
/// The identifier is the number of the output's global. The compositor never gives that number
/// to two outputs connected at the same time.
pub fn describe(info: &OutputInfo) -> MonitorInfo {
    let (width, height) = current_mode(info);
    let scale = f64::from(info.scale_factor.max(1));
    let (x, y) = info.logical_position.unwrap_or((0, 0));
    let transform = transform(info.transform);
    let mut monitor = MonitorInfo::new(
        Point::new(
            DevicePx((f64::from(x) * scale) as f32),
            DevicePx((f64::from(y) * scale) as f32),
        ),
        Size::new(DevicePx(width as f32), DevicePx(height as f32)),
        scale,
    )
    .with_id(MonitorId::new(u64::from(info.id)))
    .with_transform(transform);
    if let Some(name) = &info.name {
        monitor = monitor.with_name(name.clone());
    }
    if let Some(description) = info.description.as_ref().filter(|text| !text.is_empty()) {
        monitor = monitor.with_description(description.clone());
    }
    if !info.make.is_empty() {
        monitor.make = Some(info.make.clone().into());
    }
    if !info.model.is_empty() {
        monitor.model = Some(info.model.clone().into());
    }
    if let (Some((x, y)), Some((logical_width, logical_height))) =
        (info.logical_position, info.logical_size)
    {
        monitor = monitor.with_logical(Rect::new(
            Point::new(CssPx(x as f32), CssPx(y as f32)),
            Size::new(CssPx(logical_width as f32), CssPx(logical_height as f32)),
        ));
    }
    if let Some(fraction) = fractional_scale(info) {
        monitor = monitor.with_fractional_scale(fraction);
    }
    match refresh_rate(info) {
        Some(rate) => monitor.with_refresh_rate_millihertz(rate),
        None => monitor,
    }
}

/// The extent of the output's current mode, in its own pixels, or nothing when it states none.
fn current_mode(info: &OutputInfo) -> (i32, i32) {
    info.modes
        .iter()
        .find(|mode| mode.current)
        .map_or((0, 0), |mode| mode.dimensions)
}

/// The ratio of the output's physical width to its logical width, to the nearest 120th.
///
/// Nothing when the compositor reports no logical size.
pub fn fractional_scale(info: &OutputInfo) -> Option<f64> {
    ratio(
        current_mode(info),
        info.logical_size?,
        transform(info.transform),
    )
}

/// The ratio of a `mode` extent to a `logical` extent under `transform`, to the nearest 120th.
///
/// The mode is the extent before the transform and the logical size is the extent after it, so
/// a turned output compares its physical height with its logical width. A 120th is the unit the
/// fractional-scale protocol uses, so the answer is one the compositor can also give for a
/// surface.
fn ratio(mode: (i32, i32), logical: (i32, i32), transform: MonitorTransform) -> Option<f64> {
    let physical = if transform.swaps_axes() {
        mode.1
    } else {
        mode.0
    };
    if logical.0 <= 0 || physical <= 0 {
        return None;
    }
    let ratio = f64::from(physical) / f64::from(logical.0);
    Some((ratio * 120.0).round() / 120.0)
}

/// How the output turns its picture, in the contract's words.
pub fn transform(transform: Transform) -> MonitorTransform {
    match transform {
        Transform::_90 => MonitorTransform::Rotated90,
        Transform::_180 => MonitorTransform::Rotated180,
        Transform::_270 => MonitorTransform::Rotated270,
        Transform::Flipped => MonitorTransform::Flipped,
        Transform::Flipped90 => MonitorTransform::Flipped90,
        Transform::Flipped180 => MonitorTransform::Flipped180,
        Transform::Flipped270 => MonitorTransform::Flipped270,
        _ => MonitorTransform::Normal,
    }
}

/// The scale a surface on this output starts at.
///
/// The fraction when the surface is mapped through a viewport and the output gives one, and the
/// output's whole-number scale otherwise.
pub fn seed(info: &OutputInfo, viewport: bool) -> f64 {
    seed_of(info.scale_factor, fractional_scale(info), viewport)
}

/// The scale a surface starts at, from an output's whole-number scale and its fraction.
fn seed_of(whole: i32, fraction: Option<f64>, viewport: bool) -> f64 {
    let whole = f64::from(whole.max(1));
    match fraction {
        Some(fraction) if viewport && fraction > 0.0 => fraction,
        _ => whole,
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

/// How the colour stripes of this output are arranged, as a surface on it sees them.
///
/// The compositor states the layout of the panel, and turns the picture of the surface by the
/// transform of the output. So the transform is applied to the layout.
pub fn subpixel(info: &OutputInfo) -> SubpixelLayout {
    let panel = match info.subpixel {
        Subpixel::None | Subpixel::VerticalRgb | Subpixel::VerticalBgr => SubpixelLayout::None,
        Subpixel::HorizontalRgb => SubpixelLayout::HorizontalRgb,
        Subpixel::HorizontalBgr => SubpixelLayout::HorizontalBgr,
        _ => SubpixelLayout::Unknown,
    };
    panel.turned(transform(info.transform))
}

/// The scale of the sharpest output in `infos`, for a surface that is on all of them.
///
/// The largest rather than the first: a window straddling a 1x and a 2x monitor drawn at 1x is
/// visibly soft on half of itself, and drawn at 2x is merely oversampled on the other half.
pub fn sharpest<'a>(infos: impl Iterator<Item = &'a OutputInfo>) -> Option<i32> {
    infos.map(|info| info.scale_factor.max(1)).max()
}

#[cfg(test)]
mod tests {
    use super::{ratio, seed_of, transform};
    use smithay_client_toolkit::reexports::client::protocol::wl_output::Transform;
    use zgui_platform::MonitorTransform;

    #[test]
    fn the_fraction_is_the_physical_width_over_the_logical_width() {
        assert_eq!(
            ratio((2560, 1440), (2048, 1152), MonitorTransform::Normal),
            Some(1.25)
        );
        // 2560 / 1707 is a hair under 1.5, and the nearest 120th is 1.5 exactly.
        assert_eq!(
            ratio((2560, 1440), (1707, 960), MonitorTransform::Normal),
            Some(1.5)
        );
    }

    #[test]
    fn a_turned_output_compares_its_height_with_its_logical_width() {
        assert_eq!(
            ratio((2560, 1440), (960, 1707), MonitorTransform::Rotated90),
            Some(1.5)
        );
        assert_eq!(transform(Transform::_90), MonitorTransform::Rotated90);
        assert_eq!(
            transform(Transform::Flipped270),
            MonitorTransform::Flipped270
        );
        assert_eq!(transform(Transform::Normal), MonitorTransform::Normal);
    }

    #[test]
    fn an_output_with_no_extent_has_no_fraction() {
        assert_eq!(ratio((0, 0), (1920, 1080), MonitorTransform::Normal), None);
        assert_eq!(ratio((1920, 1080), (0, 0), MonitorTransform::Normal), None);
    }

    #[test]
    fn a_surface_without_a_viewport_starts_at_the_whole_number_scale() {
        assert_eq!(seed_of(2, Some(1.5), false), 2.0);
        assert_eq!(seed_of(2, Some(1.5), true), 1.5);
        assert_eq!(seed_of(1, None, true), 1.0);
        assert_eq!(
            seed_of(0, None, false),
            1.0,
            "a scale of zero is read as one"
        );
    }
}
