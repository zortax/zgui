//! What the device has: CRTCs, connectors, encoders and the modes they run at.

pub mod connector;
pub mod encoder;
pub mod mode;
pub mod plane;

pub use crate::resources::connector::{Connector, ConnectorKind};
pub use crate::resources::encoder::Encoder;
pub use crate::resources::mode::{Axis, Mode, ModeBuilder, ModeFlags};
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
/// [`stabilise`] is the one place that retry is written, and this is the bound it enforces.
pub(crate) const ATTEMPTS: usize = 3;

/// Runs a two-pass read, retrying while the kernel's counts move under it.
///
/// `attempt` performs one whole read and answers `None` when the counts it was handed no longer
/// match the counts the kernel reported back. That is a hot-plug between the two passes, and the
/// kernel's own instruction is to "retry the last ioctl until the number of elements stabilizes".
/// This is where that happens, once, for every list this crate reads.
///
/// `what` builds the message for a read that never settled, and runs only then.
///
/// # Errors
///
/// Returns whatever `attempt` failed with, and [`Error::Unusable`] when it never settled.
pub(crate) fn stabilise<T>(
    what: impl FnOnce() -> String,
    mut attempt: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    for _ in 0..ATTEMPTS {
        if let Some(value) = attempt()? {
            return Ok(value);
        }
    }
    Err(Error::Unusable(what()))
}

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
        stabilise(
            || "the device's resource counts changed on every attempt to read them".to_owned(),
            || {
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
                // suspect, so all of it is thrown away.
                if filled.count_fbs != counts.count_fbs
                    || filled.count_crtcs != counts.count_crtcs
                    || filled.count_connectors != counts.count_connectors
                    || filled.count_encoders != counts.count_encoders
                {
                    return Ok(None);
                }

                Ok(Some(Resources {
                    framebuffers,
                    crtcs,
                    connectors,
                    encoders,
                }))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_settles_is_the_first_one_that_answers() {
        let mut reads = 0;
        let settled = stabilise(
            || unreachable!("a read that settles never builds the message"),
            || {
                reads += 1;
                Ok((reads == 2).then_some(reads))
            },
        )
        .expect("a read that settles on the second attempt is answered");

        assert_eq!(settled, 2, "the first answer is the one handed back");
        assert_eq!(reads, 2, "a read stops as soon as it settles");
    }

    #[test]
    fn a_read_that_never_settles_gives_up_after_the_attempts_it_is_allowed() {
        let mut reads = 0;
        let error = stabilise(
            || "the counts kept moving".to_owned(),
            || {
                reads += 1;
                Ok(None::<u32>)
            },
        )
        .expect_err("a read that never settles is refused");

        assert_eq!(reads, ATTEMPTS, "every attempt is spent before giving up");
        assert!(
            matches!(&error, Error::Unusable(what) if what == "the counts kept moving"),
            "the refusal carries the message the caller built: {error}"
        );
    }
}
