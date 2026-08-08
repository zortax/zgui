//! Which displays this backend can drive.
//!
//! A display is three objects: the connector something is plugged into, the CRTC that scans a
//! picture out to it, and the plane that CRTC reads pixels from. The kernel offers the three
//! separately and says which combinations are legal. This module picks one legal combination for
//! every display it finds, which is all a commit needs.
//!
//! It also says what the contract knows about each of those displays, because a [`MonitorInfo`] is
//! read out of an output's mode and nothing else.

use tracing::warn;
use zgui_drm::commit::Pipe;
use zgui_drm::property::ObjectKind;
use zgui_drm::resources::Mode;
use zgui_drm::{Device, Error};
use zgui_geom::{DevicePx, Point, Size};
use zgui_platform::{MonitorInfo, PlatformError};

/// The plane property that says what a plane is for.
const PLANE_TYPE: &str = "type";

/// What the kernel numbers `DRM_PLANE_TYPE_PRIMARY`.
///
/// Named here because `zgui-drm` keeps the generated headers to itself: this crate reads the
/// property by name and compares the value, the way any other caller does.
const PRIMARY: u64 = 1;

/// One display this backend can drive.
#[derive(Debug, Clone)]
pub struct Output {
    /// The connector, the CRTC and the plane, as a commit names them.
    pub pipe: Pipe,
    /// The mode the display is driven at.
    pub mode: Mode,
}

impl Output {
    /// Returns every display that is plugged in, with a CRTC and a plane chosen for each.
    ///
    /// A connector with nothing plugged in is skipped. So is one the device has no free CRTC for:
    /// a device with fewer CRTCs than displays drives as many as it has and leaves the rest dark,
    /// which is better than refusing to start.
    ///
    /// [`CONNECTORS`] cuts the list down to the connectors a person named.
    ///
    /// ```no_run
    /// use zgui_drm::Device;
    /// use zgui_platform_drm::Output;
    ///
    /// let device = Device::open_first().expect("a card on this machine");
    /// let outputs = Output::discover(&device).expect("the device describes itself");
    ///
    /// for output in &outputs {
    ///     assert!(output.mode.width() > 0, "a display is driven at the extent of its mode");
    /// }
    /// let crtcs: Vec<u32> = outputs.iter().map(|output| output.pipe.crtc).collect();
    /// for (index, crtc) in crtcs.iter().enumerate() {
    ///     assert!(!crtcs[index + 1..].contains(crtc), "one CRTC scans out for one display");
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the device refuses a query, and when an atomic
    /// device gives a chosen CRTC no primary plane — a device this backend cannot drive at all.
    pub fn discover(device: &Device) -> Result<Vec<Self>, PlatformError> {
        let resources = device.resources().map_err(backend)?;
        let existing = crtc_mask(resources.crtcs.len());
        let mut primary = primary_planes(device)?;
        let mut claimed = 0;
        let mut outputs = Vec::new();

        for &connector_id in &resources.connectors {
            let connector = device.connector(connector_id).map_err(backend)?;
            if !connector.is_connected() {
                continue;
            }
            let Some(&mode) = connector.preferred_mode() else {
                continue;
            };

            // Any encoder the connector names can carry it, so the CRTCs it can be driven by are
            // the union of what those encoders reach.
            let mut reachable = 0;
            for &encoder_id in &connector.encoders {
                reachable |= device.encoder(encoder_id).map_err(backend)?.possible_crtcs;
            }

            let Some(index) = choose(reachable & existing, claimed) else {
                warn!(
                    connector = connector_id,
                    "every CRTC this connector can use is taken, so it stays dark"
                );
                continue;
            };
            claimed |= 1 << index;

            // The mask was cut to the CRTCs the device lists, so the index names one of them.
            let crtc = resources.crtcs[index as usize];
            let plane = take_plane(&mut primary, device.is_atomic(), index, crtc)?;
            outputs.push(Self {
                pipe: Pipe {
                    connector: connector_id,
                    crtc,
                    plane,
                },
                mode,
            });
        }
        Ok(outputs)
    }
}

/// Returns the index of the first CRTC an encoder can drive that nothing has claimed.
///
/// `reachable` is the encoder's mask, a bit per CRTC in the device's CRTC list. `claimed` is the
/// same shape, holding the CRTCs earlier outputs took. Answers nothing when every CRTC this
/// encoder could drive is already driving something, which is a real configuration: two displays
/// on a device with one CRTC is two displays and one picture.
fn choose(reachable: u32, claimed: u32) -> Option<u32> {
    let free = reachable & !claimed;
    (free != 0).then(|| free.trailing_zeros())
}

/// Returns a bit per CRTC the device lists, in the shape every `possible_crtcs` mask has.
///
/// A mask holds 32 bits, so a device with more CRTCs than that has some no encoder can name. A
/// chosen index is a position in the CRTC list only because the mask it came from was cut down to
/// this.
fn crtc_mask(count: usize) -> u32 {
    u32::try_from(count)
        .ok()
        .and_then(|count| 1_u32.checked_shl(count))
        .map_or(u32::MAX, |past_the_end| past_the_end - 1)
}

/// Returns every primary plane on the device, with the CRTCs it can be attached to.
///
/// Empty on a legacy device, which has no plane objects at all.
fn primary_planes(device: &Device) -> Result<Vec<(u32, u32)>, PlatformError> {
    if !device.is_atomic() {
        return Ok(Vec::new());
    }
    let mut primary = Vec::new();
    for id in device.planes().map_err(backend)? {
        let properties = device.properties(id, ObjectKind::Plane).map_err(backend)?;
        if properties.value(PLANE_TYPE) != Some(PRIMARY) {
            continue;
        }
        let plane = device.plane(id).map_err(backend)?;
        primary.push((plane.id, plane.possible_crtcs));
    }
    Ok(primary)
}

/// Returns the primary plane for the CRTC at `index`, removed from `primary` so nothing takes it
/// twice.
///
/// A plane scans out for one CRTC at a time, so a plane an earlier output took is gone from the
/// list a later one chooses from, even on a device whose planes can each reach every CRTC.
///
/// Answers zero on a legacy device, where the ioctls address the CRTC and no plane object exists.
fn take_plane(
    primary: &mut Vec<(u32, u32)>,
    is_atomic: bool,
    index: u32,
    crtc: u32,
) -> Result<u32, PlatformError> {
    if !is_atomic {
        return Ok(0);
    }
    let bit = 1 << index;
    let position = primary
        .iter()
        .position(|(_, reachable)| reachable & bit != 0)
        .ok_or_else(|| {
            PlatformError::Backend(format!(
                "CRTC {crtc} has no primary plane left to scan out from, \
                 so this device cannot be driven"
            ))
        })?;
    Ok(primary.remove(position).0)
}

/// Carries a device error across the platform boundary in the kernel's own words.
pub(crate) fn backend(error: Error) -> PlatformError {
    PlatformError::Backend(error.to_string())
}

/// Returns what the contract knows about each display, in the order the displays were found.
///
/// The conversion lives here because it is a statement about an [`Output`]: everything in a
/// [`MonitorInfo`] is read out of the mode the display is driven at.
///
/// The frame loop calls this. It discovers the outputs, describes them here, builds the surfaces
/// with [`surface::one_per_output`](crate::surface::one_per_output), and hands both to
/// [`DrmCx::new`](crate::cx::DrmCx::new).
pub fn describe(outputs: &[Output]) -> Vec<MonitorInfo> {
    outputs
        .iter()
        .map(|output| {
            describe_mode(
                output.mode.width(),
                output.mode.height(),
                output.mode.refresh_rate_millihertz(),
            )
        })
        .collect()
}

/// Returns what is known about one display, from the mode it is driven at.
///
/// The position is the origin for every display. A console has no desktop coordinate space: each
/// display is driven from its own framebuffer and nothing arranges them, so a layout here would be
/// this backend's invention rather than the machine's arrangement.
///
/// The scale factor is one, for the same reason a surface reports one: nothing states a scale, and
/// an invented one would size every application differently on every machine.
///
/// The name is left absent. The kernel calls a display after the kind of socket it is plugged into,
/// and an [`Output`] carries the connector's number rather than its kind, so there is nothing here
/// to name it with.
fn describe_mode(width: u32, height: u32, millihertz: u32) -> MonitorInfo {
    let monitor = MonitorInfo::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(width as f32), DevicePx(height as f32)),
        1.0,
    );
    // A rate of zero is a mode whose timings give none, and inventing sixty here would hide that
    // from the fallback the contract states once and applies everywhere.
    if millihertz > 0 {
        monitor.with_refresh_rate_millihertz(millihertz)
    } else {
        monitor
    }
}

#[cfg(test)]
mod tests {
    use super::{Output, choose, crtc_mask, describe, describe_mode};
    use zgui_drm::commit::Pipe;
    use zgui_drm::resources::{Axis, Mode};
    use zgui_geom::{DevicePx, Point, Size};

    /// An output on `crtc`, driven at `width` x `height` at 60 Hz.
    ///
    /// The timings are those of a real 1920x1080 at 60 Hz scaled to the extent asked for. The
    /// blanking carries the rate, so a mode built with a plausible one is described with a
    /// plausible rate.
    fn output(crtc: u32, width: u16, height: u16) -> Output {
        let horizontal = Axis {
            visible: width,
            sync_start: width + 88,
            sync_end: width + 132,
            total: width + 280,
        };
        let vertical = Axis {
            visible: height,
            sync_start: height + 4,
            sync_end: height + 9,
            total: height + 45,
        };
        // A pixel clock of one kilohertz per whole scan gives exactly 1000 Hz over that scan, so
        // sixty of those is 60 Hz whatever the extent.
        let clock = 60 * u32::from(horizontal.total) * u32::from(vertical.total) / 1_000;
        Output {
            pipe: Pipe {
                connector: crtc + 1,
                crtc,
                plane: crtc + 2,
            },
            mode: Mode::builder(clock, horizontal, vertical).build(),
        }
    }

    #[test]
    fn the_lowest_free_crtc_is_taken() {
        assert_eq!(choose(0b1111, 0b0001), Some(1));
        assert_eq!(choose(0b1010, 0), Some(1));
    }

    #[test]
    fn a_mask_with_no_free_crtc_answers_nothing() {
        assert_eq!(choose(0b0101, 0b0101), None);
        assert_eq!(choose(0b0101, 0b1111), None);
    }

    #[test]
    fn an_encoder_that_reaches_no_crtc_answers_nothing() {
        assert_eq!(choose(0, 0), None);
    }

    #[test]
    fn claiming_in_sequence_hands_out_distinct_crtcs_until_they_run_out() {
        let mut claimed = 0;
        let mut chosen = Vec::new();
        for _ in 0..4 {
            let Some(index) = choose(0b0111, claimed) else {
                break;
            };
            claimed |= 1 << index;
            chosen.push(index);
        }
        assert_eq!(chosen, [0, 1, 2], "three CRTCs are handed out once each");
        assert_eq!(choose(0b0111, claimed), None, "and then there are none");
    }

    #[test]
    fn a_mask_covers_the_crtcs_the_device_lists() {
        assert_eq!(crtc_mask(0), 0);
        assert_eq!(crtc_mask(4), 0b1111);
        assert_eq!(crtc_mask(31), u32::MAX >> 1);
        assert_eq!(crtc_mask(32), u32::MAX);
        assert_eq!(crtc_mask(64), u32::MAX, "a mask holds no more than 32 bits");
    }

    #[test]
    fn a_display_is_described_from_the_mode_it_runs_at() {
        let monitor = describe_mode(1920, 1080, 60_000);
        assert_eq!(
            monitor.size,
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
            "a display is the extent of its mode"
        );
        assert_eq!(monitor.refresh_rate_millihertz, Some(60_000));
        assert_eq!(monitor.scale_factor, 1.0);
        assert_eq!(monitor.position, Point::new(DevicePx(0.0), DevicePx(0.0)));
        assert_eq!(monitor.name, None);
    }

    #[test]
    fn a_mode_whose_timings_give_no_rate_reports_none() {
        assert_eq!(describe_mode(1920, 1080, 0).refresh_rate_millihertz, None);
    }

    #[test]
    fn a_device_with_no_display_is_described_by_nothing() {
        assert!(describe(&[]).is_empty());
    }

    #[test]
    fn every_display_is_described_from_its_own_mode_in_the_order_it_was_found() {
        // Two displays of different extents, so a description built from the wrong output's mode
        // shows up as an extent that belongs to the other one.
        let outputs = [output(62, 1920, 1080), output(81, 1280, 720)];
        let monitors = describe(&outputs);

        assert_eq!(monitors.len(), 2, "every display is described once");
        assert_eq!(
            monitors[0].size,
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
            "the first display is the extent of the first mode"
        );
        assert_eq!(
            monitors[1].size,
            Size::new(DevicePx(1280.0), DevicePx(720.0)),
            "and the second of the second"
        );
        for monitor in &monitors {
            assert_eq!(
                monitor.refresh_rate_millihertz,
                Some(60_000),
                "both modes were built at 60 Hz"
            );
            assert_eq!(
                monitor.position,
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                "a console arranges nothing, so every display is at the origin"
            );
        }
    }
}
