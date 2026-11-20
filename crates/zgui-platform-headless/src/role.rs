//! What the headless platform makes of each surface role.
//!
//! Every role is an offscreen surface here, so a test can drive a pop-up, a shell layer and a
//! lock window the way it drives an ordinary window. The platform does what a desktop decides
//! about the surface's existence and its size, and records the rest:
//!
//! | Role | Refused when | Extent |
//! |---|---|---|
//! | toplevel | never | the requested size |
//! | pop-up | the parent surface does not exist | the requested size |
//! | shell layer | the platform reports outputs and none matches the requested one | the requested size, stretched across the output between two opposite anchors |
//! | lock | never | the output's size |
//!
//! The output is the one the role names, else the first one the platform reports. A platform that
//! reports no output gives a stretched axis and a lock window the default extent. The placement is
//! not applied to a position: a test reads it back through
//! [`OffscreenSurface::layer_placement`](crate::surface::OffscreenSurface::layer_placement) and
//! [`OffscreenSurface::popup_placement`](crate::surface::OffscreenSurface::popup_placement).

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::{
    LayerPlacement, MonitorInfo, MonitorTarget, PlatformError, SurfaceAttributes, SurfaceId,
    SurfaceRole,
};

/// The extent `attributes` gives a surface on a platform reporting `monitors`, or why the
/// platform refuses it.
///
/// `exists` answers whether a surface with an identifier exists, for the parent of a pop-up.
pub(crate) fn extent(
    attributes: &SurfaceAttributes,
    monitors: &[MonitorInfo],
    default: Size<DevicePx, Device>,
    exists: impl Fn(SurfaceId) -> bool,
) -> Result<Size<DevicePx, Device>, PlatformError> {
    let requested = attributes.size.map_or(default, |size| {
        Size::new(DevicePx(size.width.0), DevicePx(size.height.0))
    });
    match &attributes.role {
        SurfaceRole::Popup(placement) if !exists(placement.parent) => {
            Err(PlatformError::NoSuchSurface)
        }
        SurfaceRole::Layer(placement) => {
            let output = output(placement.monitor.as_ref(), monitors)?;
            Ok(stretched(requested, placement, output, default))
        }
        SurfaceRole::Lock(target) => Ok(output(Some(target), monitors)?.map_or(default, css_size)),
        _ => Ok(requested),
    }
}

/// The output `target` names among `monitors`, else the first one.
///
/// Refuses a target that matches none of the outputs a platform reports. A platform that reports
/// none has no output to refuse against.
fn output<'a>(
    target: Option<&MonitorTarget>,
    monitors: &'a [MonitorInfo],
) -> Result<Option<&'a MonitorInfo>, PlatformError> {
    let Some(target) = target else {
        return Ok(monitors.first());
    };
    if monitors.is_empty() {
        return Ok(None);
    }
    monitors
        .iter()
        .find(|monitor| target.matches(monitor))
        .map(Some)
        .ok_or_else(|| {
            PlatformError::SurfaceCreation(format!("no connected output matches {target:?}"))
        })
}

/// The size of `monitor` in CSS pixels, which the headless surface draws at one to one.
fn css_size(monitor: &MonitorInfo) -> Size<DevicePx, Device> {
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor as f32
    } else {
        1.0
    };
    Size::new(
        DevicePx(monitor.size.width.0 / scale),
        DevicePx(monitor.size.height.0 / scale),
    )
}

/// `requested`, stretched across `output` on each axis `placement` anchors at both ends.
fn stretched(
    requested: Size<DevicePx, Device>,
    placement: &LayerPlacement,
    output: Option<&MonitorInfo>,
    default: Size<DevicePx, Device>,
) -> Size<DevicePx, Device> {
    let across = output.map_or(default, css_size);
    let anchors = placement.anchors;
    let margin = placement.margin;
    let mut size = requested;
    if anchors.left && anchors.right {
        size.width = DevicePx((across.width.0 - margin.left.0 - margin.right.0).max(0.0));
    }
    if anchors.top && anchors.bottom {
        size.height = DevicePx((across.height.0 - margin.top.0 - margin.bottom.0).max(0.0));
    }
    size
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Device, DevicePx, Edges, Point, Rect, Size};
    use zgui_platform::{
        Layer, LayerPlacement, MonitorId, MonitorInfo, MonitorTarget, PlatformError,
        PopupPlacement, SurfaceAttributes, SurfaceId, SurfaceRole,
    };

    use super::extent;

    const DEFAULT: Size<DevicePx, Device> = Size::new(DevicePx(640.0), DevicePx(480.0));

    fn output(id: u64, width: f32, height: f32, scale: f64) -> MonitorInfo {
        MonitorInfo::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(height)),
            scale,
        )
        .with_id(MonitorId::new(id))
    }

    fn with_role(role: SurfaceRole, size: Option<(f32, f32)>) -> SurfaceAttributes {
        let mut attributes = SurfaceAttributes::new("test");
        attributes.role = role;
        attributes.size = size.map(|(width, height)| Size::new(CssPx(width), CssPx(height)));
        attributes
    }

    #[test]
    fn a_bar_stretches_across_its_output_between_its_margins() {
        let bar = LayerPlacement::new(Layer::Top)
            .with_anchors(Edges::new(true, true, false, true))
            .with_margin(Edges::new(CssPx(0.0), CssPx(8.0), CssPx(0.0), CssPx(8.0)))
            .with_monitor(MonitorId::new(2));
        let attributes = with_role(SurfaceRole::Layer(bar), Some((0.0, 34.0)));
        let monitors = [
            output(1, 1920.0, 1080.0, 1.0),
            output(2, 2560.0, 1440.0, 2.0),
        ];
        let size = extent(&attributes, &monitors, DEFAULT, |_| true).expect("accepted");
        assert_eq!(size, Size::new(DevicePx(1264.0), DevicePx(34.0)));
    }

    #[test]
    fn a_layer_on_an_output_that_is_not_there_is_refused() {
        let placement = LayerPlacement::new(Layer::Top).with_monitor(MonitorId::new(9));
        let attributes = with_role(SurfaceRole::Layer(placement), Some((10.0, 10.0)));
        let refused = extent(
            &attributes,
            &[output(1, 800.0, 600.0, 1.0)],
            DEFAULT,
            |_| true,
        );
        assert!(matches!(refused, Err(PlatformError::SurfaceCreation(_))));
        // With no outputs reported there is nothing to refuse against.
        assert!(extent(&attributes, &[], DEFAULT, |_| true).is_ok());
    }

    #[test]
    fn a_pop_up_needs_its_parent() {
        let rect: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(0.0), CssPx(0.0)),
            Size::new(CssPx(10.0), CssPx(10.0)),
        );
        let popup = PopupPlacement::below(SurfaceId::new(4), rect);
        let attributes = with_role(SurfaceRole::Popup(popup), Some((120.0, 80.0)));
        assert_eq!(
            extent(&attributes, &[], DEFAULT, |id| id == SurfaceId::new(4)),
            Ok(Size::new(DevicePx(120.0), DevicePx(80.0)))
        );
        assert_eq!(
            extent(&attributes, &[], DEFAULT, |_| false),
            Err(PlatformError::NoSuchSurface)
        );
    }

    #[test]
    fn a_lock_window_covers_its_output() {
        let attributes = with_role(
            SurfaceRole::Lock(MonitorTarget::Id(MonitorId::new(1))),
            None,
        );
        let size = extent(
            &attributes,
            &[output(1, 1920.0, 1080.0, 1.0)],
            DEFAULT,
            |_| true,
        );
        assert_eq!(size, Ok(Size::new(DevicePx(1920.0), DevicePx(1080.0))));
        assert_eq!(extent(&attributes, &[], DEFAULT, |_| true), Ok(DEFAULT));
    }
}
