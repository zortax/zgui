//! The hardware cursor: a small buffer on a plane of its own.
//!
//! A pointer drawn into the frame costs a redraw of everything under it every time it moves. A
//! hardware cursor costs one property commit: the image sits on a plane the display engine
//! composites on its own, and moving it changes two numbers. This module answers what the device
//! offers — the size it wants and the plane it has — and [`Commit`](crate::Commit) is where the
//! image is put on, moved and taken away.
//!
//! On the atomic interface a device with no cursor plane leaves the caller to draw the pointer
//! into the frame itself, and [`Device::cursor_plane`] answering `None` says so. The legacy
//! request names the CRTC and needs no plane.

use crate::device::Device;
use crate::error::Result;
use crate::framebuffer::Framebuffer;
use crate::property::ObjectKind;
use crate::sys;

/// What a plane's `type` property answers for a cursor plane.
///
/// The vendored headers do not declare this. The values of `type` are the kernel's `enum
/// drm_plane_type`, which lives in an internal header, so the number is transcribed here and held
/// by a test that reads the property's own enumeration off a device and checks the entry named
/// `Cursor` against it.
const CURSOR_PLANE_TYPE: u64 = 2;

/// The size of the buffer a device wants a cursor in, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorSize {
    /// How wide.
    pub width: u32,
    /// How tall.
    pub height: u32,
}

impl CursorSize {
    /// 64×64, the size a driver that answers neither capability wants.
    ///
    /// Every driver took this size before `DRM_CAP_CURSOR_WIDTH` and `DRM_CAP_CURSOR_HEIGHT`
    /// existed, so it is the historical default.
    pub const DEFAULT: Self = Self {
        width: 64,
        height: 64,
    };

    /// Returns the size two capability answers describe, defaulting each axis the driver said
    /// nothing usable about.
    ///
    /// DRM core answers these two capabilities on the driver's behalf, and it substitutes 64 where
    /// a driver states no cursor extent. So the reachable `None` is a node that serves no
    /// modesetting at all and refuses the query. Zero, and a value past a `u32`, are answers this
    /// crate has never seen: each would otherwise become a buffer extent of zero or a truncated
    /// one, so each takes the default too.
    fn from_capabilities(width: Option<u64>, height: Option<u64>) -> Self {
        fn extent(reported: Option<u64>, default: u32) -> u32 {
            reported
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value != 0)
                .unwrap_or(default)
        }

        Self {
            width: extent(width, Self::DEFAULT.width),
            height: extent(height, Self::DEFAULT.height),
        }
    }
}

/// A cursor image, named the way both interfaces need it.
///
/// The two interfaces name the same buffer differently, and neither name can be derived from the
/// other here. An atomic commit sets the cursor plane's `FB_ID`, which is a framebuffer id.
/// `DRM_IOCTL_MODE_CURSOR2` names the GEM handle the driver allocated the buffer as, and states
/// its extent beside it. So both travel, and the interface decides which one is read. A caller
/// holding a dumb buffer has both already: [`DumbBuffer::handle`] and the [`Framebuffer`] that
/// [`Device::add_framebuffer`] made from it.
///
/// [`DumbBuffer::handle`]: crate::buffer::DumbBuffer::handle
#[derive(Debug, Clone, Copy)]
pub struct CursorImage {
    /// The framebuffer the atomic interface scans the image out of.
    pub framebuffer: Framebuffer,
    /// The GEM handle the legacy interface names the same buffer by.
    pub handle: u32,
    /// How wide the image is, in pixels.
    ///
    /// The legacy request carries the extent and the handle and nothing else — no stride and no
    /// format — so the buffer has to hold exactly this. Allocating at [`Device::cursor_size`] is
    /// what keeps that true.
    pub width: u32,
    /// How tall the image is, in pixels.
    pub height: u32,
    /// Where in the image the pointer points, in pixels right of its left edge.
    ///
    /// A position is the image's top left corner on both interfaces, so a caller puts the image at
    /// the pointer less the hotspot. The hotspot also travels to the driver, and
    /// `DRM_IOCTL_MODE_CURSOR2` is the only request with a field for it: a para-virtualised driver
    /// relays it to the host that draws the pointer.
    ///
    /// The atomic property set has no standard equivalent, so the atomic path drops it and sends
    /// the position alone. `HOTSPOT_X` and `HOTSPOT_Y` exist on those same para-virtualised
    /// drivers behind `DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT`, which this crate does not ask for.
    /// On the atomic interface the kernel hides a cursor plane from a client that did not ask, so
    /// [`Device::cursor_plane`] finds none on those drivers. A legacy client is served by
    /// `DRM_IOCTL_MODE_CURSOR2`, which carries the hotspot and reads no plane.
    pub hotspot_x: i32,
    /// Where in the image the pointer points, in pixels below its top edge.
    ///
    /// Carried the way [`CursorImage::hotspot_x`] is.
    pub hotspot_y: i32,
}

/// Where a cursor goes: the CRTC that shows it, and the plane the atomic interface puts it on.
#[derive(Debug, Clone, Copy)]
pub struct CursorPlane {
    /// The CRTC showing the cursor.
    ///
    /// The legacy interface names this and reads nothing else about the target.
    pub crtc: u32,
    /// The plane id, from [`Device::cursor_plane`].
    ///
    /// Zero where the device offers no cursor plane. The atomic interface refuses that, because a
    /// plane id is the only way it can name a cursor at all.
    pub id: u32,
}

impl Device {
    /// Returns the size this device wants a cursor buffer in.
    ///
    /// A driver that answers neither capability wants [`CursorSize::DEFAULT`], which is 64×64.
    ///
    /// The cross-driver contract is that the answer is *a* size that works. A driver may mean more
    /// by it — i915 answers with the largest plane it has — so this is a size to allocate. A
    /// buffer of another size is refused by some drivers and scanned out as noise by others.
    pub fn cursor_size(&self) -> CursorSize {
        CursorSize::from_capabilities(
            self.capability(u64::from(sys::DRM_CAP_CURSOR_WIDTH)).ok(),
            self.capability(u64::from(sys::DRM_CAP_CURSOR_HEIGHT)).ok(),
        )
    }

    /// The cursor plane that can drive the CRTC at `crtc_index`, where this device has one.
    ///
    /// `crtc_index` is a place in [`Resources::crtcs`](crate::resources::Resources::crtcs) rather
    /// than a CRTC id, because that is what
    /// [`Plane::possible_crtcs`](crate::resources::Plane::possible_crtcs) indexes. A device
    /// usually carries one cursor plane per CRTC, so the mask is what tells them apart.
    ///
    /// What makes a plane a cursor plane is the value of its `type` property, which is read here
    /// the way any other property is read.
    ///
    /// Answers `None` where the device offers no cursor plane for that CRTC, and a caller that
    /// gets `None` draws the pointer into the frame itself. A device opened for the legacy
    /// interface always answers `None`: the kernel hides primary and cursor planes from a client
    /// that did not ask for universal planes, and the legacy cursor request names the CRTC and
    /// needs no plane.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses a read, and
    /// [`Error::Unusable`](crate::Error::Unusable) when a count kept moving under one.
    pub fn cursor_plane(&self, crtc_index: usize) -> Result<Option<u32>> {
        for id in self.planes()? {
            if !self.plane(id)?.drives(crtc_index) {
                continue;
            }
            if self.properties(id, ObjectKind::Plane)?.value("type") == Some(CURSOR_PLANE_TYPE) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    //! What a cursor size defaults to, and what the kernel calls a cursor plane.
    //!
    //! The defaulting is pure. The plane type is the one number in this file that no vendored
    //! header declares, so it is checked against a device where there is one.

    use super::*;

    use crate::ioctl;

    #[test]
    fn a_driver_that_answers_nothing_wants_the_size_every_driver_wanted_first() {
        assert_eq!(
            CursorSize::from_capabilities(None, None),
            CursorSize::DEFAULT
        );
        assert_eq!(CursorSize::DEFAULT.width, 64);
        assert_eq!(CursorSize::DEFAULT.height, 64);
    }

    #[test]
    fn a_driver_that_answers_is_taken_at_its_word() {
        assert_eq!(
            CursorSize::from_capabilities(Some(256), Some(128)),
            CursorSize {
                width: 256,
                height: 128
            }
        );
    }

    #[test]
    fn an_axis_the_driver_said_nothing_usable_about_defaults_on_its_own() {
        // Zero describes no buffer, and an extent past a `u32` is one no framebuffer can have.
        // Each axis falls back by itself, so an answer for one is kept while the other defaults.
        assert_eq!(
            CursorSize::from_capabilities(Some(0), Some(128)),
            CursorSize {
                width: 64,
                height: 128
            }
        );
        assert_eq!(
            CursorSize::from_capabilities(Some(256), Some(u64::from(u32::MAX) + 1)),
            CursorSize {
                width: 256,
                height: 64
            }
        );
        assert_eq!(
            CursorSize::from_capabilities(Some(256), None),
            CursorSize {
                width: 256,
                height: 64
            }
        );
    }

    #[test]
    fn the_plane_type_a_cursor_plane_reports_is_the_one_the_kernel_names_cursor() {
        let Ok(device) = Device::open_first() else {
            eprintln!(
                "the_plane_type_a_cursor_plane_reports_is_the_one_the_kernel_names_cursor: no DRM \
                 device on this machine, so nothing was asserted\n\
                 load the virtual driver with `sudo modprobe vkms` to run it"
            );
            return;
        };
        let Some(plane) = device
            .planes()
            .ok()
            .and_then(|planes| planes.first().copied())
        else {
            eprintln!("this device lists no plane, so nothing was asserted");
            return;
        };
        let Some(property) = device
            .properties(plane, ObjectKind::Plane)
            .ok()
            .and_then(|properties| properties.id("type"))
        else {
            eprintln!("this device's planes state no type, so nothing was asserted");
            return;
        };

        let named = enumeration(&device, property);
        let cursor = named
            .iter()
            .find(|(name, _)| name == "Cursor")
            .map(|(_, value)| *value);
        assert_eq!(
            cursor,
            Some(CURSOR_PLANE_TYPE),
            "the kernel's own name for the plane type this file transcribes, out of {named:?}"
        );
    }

    /// Returns the `(name, value)` pairs of an enumerated property.
    ///
    /// The values of `type` come from a header this crate does not vendor, so this reads the names
    /// the kernel puts beside them. Two passes, the way the header asks: the first for the count,
    /// the second for the entries.
    fn enumeration(device: &Device, property: u32) -> Vec<(String, u64)> {
        let mut counts = sys::drm_mode_get_property {
            prop_id: property,
            ..Default::default()
        };
        ioctl::issue(device.fd(), ioctl::MODE_GETPROPERTY, &mut counts)
            .expect("a property named by an object is readable");

        let mut entries =
            vec![sys::drm_mode_property_enum::default(); counts.count_enum_blobs as usize];
        let mut filled = sys::drm_mode_get_property {
            prop_id: property,
            enum_blob_ptr: entries.as_mut_ptr() as u64,
            count_enum_blobs: counts.count_enum_blobs,
            ..Default::default()
        };
        ioctl::issue(device.fd(), ioctl::MODE_GETPROPERTY, &mut filled)
            .expect("a property named by an object is readable");

        entries
            .iter()
            .map(|entry| {
                let name: Vec<u8> = entry
                    .name
                    .iter()
                    .take_while(|byte| **byte != 0)
                    // `c_char` is signed on x86_64 and unsigned on aarch64, so this cast converts
                    // on the first and is the no-op `unnecessary_cast` rejects on the second.
                    .map(|byte| *byte as u8)
                    .collect();
                (String::from_utf8_lossy(&name).into_owned(), entry.value)
            })
            .collect()
    }
}
