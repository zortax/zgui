//! What the device has: CRTCs, connectors, encoders and the modes they run at.

pub mod connector;
pub mod plane;

pub use crate::resources::connector::{Connector, ConnectorKind, Mode};
pub use crate::resources::plane::Plane;

use crate::device::Device;
use crate::error::{Error, Result};
use crate::ioctl;
use crate::sys;

/// How many times an enumeration is retried when the counts move under it.
///
/// The kernel reports how many objects there are, and is then asked again with buffers that size.
/// A monitor plugged in between the two changes the answer. Three attempts covers that race and
/// stops short of a livelock: a device whose counts change three times in a row is one nothing can
/// enumerate.
///
/// The retry is the kernel's own protocol. The header for `drm_mode_get_connector` states that
/// performing the ioctl twice may be racy, and that user space is expected to repeat the last call
/// until the number of elements stabilises.
///
/// [`Device::resources`] and [`Device::connector`] both make that two-pass read, and this is the
/// bound both give up at.
pub(crate) const ATTEMPTS: usize = 3;

/// Everything the device listed.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Resources {
    /// The framebuffer ids currently registered.
    pub framebuffers: Vec<u32>,
    /// The CRTCs, which are what drives a display.
    pub crtcs: Vec<u32>,
    /// The connectors, which are where a display is plugged in.
    pub connectors: Vec<u32>,
    /// The encoders, which sit between the two.
    pub encoders: Vec<u32>,
}

impl Device {
    /// Returns everything this device has.
    ///
    /// Every other read starts from these lists: a connector id goes to [`Device::connector`], an
    /// encoder id to [`Device::encoder`], and a plane's or an encoder's `possible_crtcs` mask
    /// indexes [`Resources::crtcs`] by place.
    ///
    /// ```no_run
    /// use zgui_drm::Device;
    ///
    /// let device = Device::open_first()?;
    /// let resources = device.resources()?;
    ///
    /// for id in resources.connectors {
    ///     assert_eq!(
    ///         device.connector(id)?.id,
    ///         id,
    ///         "every id this listed names a connector the device will describe",
    ///     );
    /// }
    /// # Ok::<(), zgui_drm::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses, and [`Error::Unusable`] when the counts
    /// kept moving.
    pub fn resources(&self) -> Result<Resources> {
        for _ in 0..ATTEMPTS {
            // First pass: no buffers, so the kernel fills in the counts and writes nothing.
            let mut counts = sys::drm_mode_card_res::default();
            ioctl::issue(self.fd(), ioctl::MODE_GETRESOURCES, &mut counts)?;

            let mut framebuffers = vec![0_u32; counts.count_fbs as usize];
            let mut crtcs = vec![0_u32; counts.count_crtcs as usize];
            let mut connectors = vec![0_u32; counts.count_connectors as usize];
            let mut encoders = vec![0_u32; counts.count_encoders as usize];

            let mut filled = sys::drm_mode_card_res {
                fb_id_ptr: framebuffers.as_mut_ptr() as u64,
                crtc_id_ptr: crtcs.as_mut_ptr() as u64,
                connector_id_ptr: connectors.as_mut_ptr() as u64,
                encoder_id_ptr: encoders.as_mut_ptr() as u64,
                count_fbs: counts.count_fbs,
                count_crtcs: counts.count_crtcs,
                count_connectors: counts.count_connectors,
                count_encoders: counts.count_encoders,
                ..Default::default()
            };
            ioctl::issue(self.fd(), ioctl::MODE_GETRESOURCES, &mut filled)?;

            // Something was plugged in or unplugged between the passes. Everything read is
            // suspect, so it is thrown away rather than truncated.
            if filled.count_fbs != counts.count_fbs
                || filled.count_crtcs != counts.count_crtcs
                || filled.count_connectors != counts.count_connectors
                || filled.count_encoders != counts.count_encoders
            {
                continue;
            }

            return Ok(Resources {
                framebuffers,
                crtcs,
                connectors,
                encoders,
            });
        }

        Err(Error::Unusable(
            "the device's resource counts changed on every attempt to read them".to_owned(),
        ))
    }
}
