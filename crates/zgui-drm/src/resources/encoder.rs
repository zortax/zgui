//! An encoder: what carries a CRTC's picture to a connector.

use crate::device::Device;
use crate::error::Result;
use crate::ioctl;
use crate::sys;

// What kind of encoder this is — TMDS, DSI, a DisplayPort MST branch — is the value of
// `encoder_type`, and nothing routes on it: a caller picks a CRTC out of `possible_crtcs`. An
// enumeration here would be a second spelling of a number nobody reads, and `cargo xtask ledger
// inert` would fail it.
//
// `possible_clones` is left out for the same reason. The kernel describes it as a bitmask of the
// sibling encoders this one can be cloned with, and cloning one picture onto two connectors is not
// something this crate does.

/// One encoder.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Encoder {
    /// The object id, as a connector names it.
    pub id: u32,
    /// A bit per CRTC in the device's CRTC list, set where this encoder can be driven by it.
    ///
    /// Bit N is the CRTC at place N in [`Resources::crtcs`](crate::resources::Resources::crtcs).
    /// The uapi header leaves the encoder mask undocumented; `drm_encoder.h` states it as
    /// `drm_crtc_index()` indexing the bitfield, and that index is a CRTC's place in that list.
    pub possible_crtcs: u32,
    /// The CRTC currently driving it, when there is one.
    pub crtc: Option<u32>,
}

impl Device {
    /// Reads the encoder with this id.
    ///
    /// This is how a connector and a CRTC are known to be routable together: the connector names
    /// its encoders, and an encoder names the CRTCs it reaches. A pair chosen without it sets a
    /// mode the hardware cannot wire up, and the kernel refuses that with `EINVAL` and nothing
    /// more.
    ///
    /// ```no_run
    /// use zgui_drm::Device;
    ///
    /// let device = Device::open_first()?;
    ///
    /// for id in device.resources()?.connectors {
    ///     let connector = device.connector(id)?;
    ///     let Some(attached) = connector.encoder else {
    ///         continue;
    ///     };
    ///     assert!(
    ///         connector.encoders.contains(&attached),
    ///         "the encoder driving a connector is one of the encoders it lists",
    ///     );
    ///     assert_eq!(device.encoder(attached)?.id, attached);
    /// }
    /// # Ok::<(), zgui_drm::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses.
    pub fn encoder(&self, id: u32) -> Result<Encoder> {
        // One pass: the kernel's structure is five plain numbers with no array behind a pointer,
        // so there is no count to read first and nothing that can move between two reads.
        let mut encoder = sys::drm_mode_get_encoder {
            encoder_id: id,
            ..Default::default()
        };
        ioctl::issue(self.fd(), ioctl::MODE_GETENCODER, &mut encoder)?;

        Ok(Encoder {
            id,
            possible_crtcs: encoder.possible_crtcs,
            crtc: (encoder.crtc_id != 0).then_some(encoder.crtc_id),
        })
    }
}
