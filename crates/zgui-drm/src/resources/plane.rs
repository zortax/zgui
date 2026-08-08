//! A plane: the thing a framebuffer is actually scanned out from.

use crate::device::Device;
use crate::error::Result;
use crate::ioctl;
use crate::resources::stabilise;
use crate::sys;

// What a plane is *for* — primary, overlay or cursor — is the value of its `type` property, and
// `property` already reads any property of any object. An enumeration here would be a second
// spelling of that value, and `cargo xtask ledger inert` would fail it: nothing would construct
// the variants. A caller that needs the kind reads the property.

/// One plane.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Plane {
    /// The object id, for naming it in a commit.
    pub id: u32,
    /// A bit per CRTC in the device's CRTC list, set where this plane can drive it.
    ///
    /// The header states it as bit N naming the CRTC at index N, which is its place in
    /// [`Resources::crtcs`](crate::resources::Resources::crtcs). [`Plane::drives`] reads it.
    pub possible_crtcs: u32,
    /// The CRTC currently driving it, when there is one.
    pub crtc: Option<u32>,
    /// The formats it can scan out, as fourcc codes.
    pub formats: Vec<u32>,
}

impl Device {
    /// Returns the plane ids this device has.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses, and
    /// [`Error::Unusable`](crate::Error::Unusable) when the count kept moving.
    pub fn planes(&self) -> Result<Vec<u32>> {
        stabilise(
            || "the device's plane count changed on every attempt to read it".to_owned(),
            || {
                let mut counts = sys::drm_mode_get_plane_res::default();
                ioctl::issue(self.fd(), ioctl::MODE_GETPLANERESOURCES, &mut counts)?;

                let mut ids = vec![0_u32; counts.count_planes as usize];
                let mut filled = sys::drm_mode_get_plane_res {
                    plane_id_ptr: ids.as_mut_ptr() as u64,
                    count_planes: counts.count_planes,
                };
                ioctl::issue(self.fd(), ioctl::MODE_GETPLANERESOURCES, &mut filled)?;

                if filled.count_planes != counts.count_planes {
                    return Ok(None);
                }
                Ok(Some(ids))
            },
        )
    }

    /// Reads the plane with this id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses, and
    /// [`Error::Unusable`](crate::Error::Unusable) when the count kept moving.
    pub fn plane(&self, id: u32) -> Result<Plane> {
        stabilise(
            || format!("plane {id} changed under every attempt to read it"),
            || {
                let mut counts = sys::drm_mode_get_plane {
                    plane_id: id,
                    ..Default::default()
                };
                ioctl::issue(self.fd(), ioctl::MODE_GETPLANE, &mut counts)?;

                let mut formats = vec![0_u32; counts.count_format_types as usize];
                let mut filled = sys::drm_mode_get_plane {
                    plane_id: id,
                    format_type_ptr: formats.as_mut_ptr() as u64,
                    count_format_types: counts.count_format_types,
                    ..Default::default()
                };
                ioctl::issue(self.fd(), ioctl::MODE_GETPLANE, &mut filled)?;

                if filled.count_format_types != counts.count_format_types {
                    return Ok(None);
                }

                Ok(Some(Plane {
                    id,
                    possible_crtcs: filled.possible_crtcs,
                    crtc: (filled.crtc_id != 0).then_some(filled.crtc_id),
                    formats,
                }))
            },
        )
    }
}
