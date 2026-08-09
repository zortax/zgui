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

impl Plane {
    /// Returns `true` when this plane can drive the CRTC at `crtc_index`.
    ///
    /// `crtc_index` is a place in [`Resources::crtcs`](crate::resources::Resources::crtcs), the
    /// list [`Plane::possible_crtcs`] indexes. A caller that passed a CRTC id would get an answer
    /// about whichever CRTC happens to sit at that place.
    ///
    /// The mask is one `u32`, so it describes at most 32 CRTCs and an index past that is answered
    /// with false. Rust leaves a shift by 32 or more undefined, and a debug build panics on it.
    pub fn drives(&self, crtc_index: usize) -> bool {
        u32::try_from(crtc_index)
            .ok()
            .filter(|index| *index < u32::BITS)
            .is_some_and(|index| self.possible_crtcs & (1 << index) != 0)
    }
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

#[cfg(test)]
mod tests {
    //! Which CRTCs a plane says it can drive.
    //!
    //! The mask indexes the CRTC list, so this is pure and needs no device.

    use super::*;

    /// Returns a plane with `possible_crtcs` and nothing else that this asks about.
    fn plane(possible_crtcs: u32) -> Plane {
        Plane {
            id: 1,
            possible_crtcs,
            crtc: None,
            formats: Vec::new(),
        }
    }

    #[test]
    fn a_plane_drives_the_crtcs_its_mask_names_by_their_place_in_the_list() {
        // The second and the fourth CRTC of the device's list.
        let plane = plane(0b1010);

        assert!(!plane.drives(0), "bit 0 is clear, so the first CRTC is out");
        assert!(plane.drives(1), "bit 1 is set, so the second CRTC is in");
        assert!(!plane.drives(2), "bit 2 is clear, so the third CRTC is out");
        assert!(plane.drives(3), "bit 3 is set, so the fourth CRTC is in");
    }

    #[test]
    fn a_plane_that_names_no_crtc_drives_none() {
        assert!(!plane(0).drives(0));
    }

    #[test]
    fn an_index_past_the_mask_is_answered_rather_than_shifted_off_the_end() {
        // A shift of 32 or more is undefined in Rust, and a debug build panics on it. So this is
        // the assertion that a caller counting CRTCs wrong gets an answer.
        let plane = plane(u32::MAX);

        assert!(plane.drives(31), "the last bit the mask has is readable");
        assert!(!plane.drives(32), "one past the mask names no CRTC");
        assert!(!plane.drives(usize::MAX), "and neither does anything above");
    }
}
