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
use crate::format::Format;
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
    /// modesetting at all and refuses the query, and the two branches below it — zero, and a value
    /// past a `u32` — guard answers this crate has never seen. They stay because each would
    /// otherwise become a buffer extent of zero or a truncated one, and both are silent.
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
/// `DRM_IOCTL_MODE_CURSOR2` names the GEM handle the driver allocated the buffer as. So both
/// travel, and the interface decides which one is read. A caller holding a dumb buffer has both
/// already: [`DumbBuffer::handle`] and the [`Framebuffer`] that [`Device::add_framebuffer`] made
/// from it.
///
/// # What the legacy interface substitutes
///
/// `drm_mode_cursor2` carries an extent and a handle. It carries no format and no stride, and the
/// kernel fills both in for itself: it reads the buffer as [`CursorImage::LEGACY_FORMAT`], with
/// rows of [`CursorImage::legacy_stride`] bytes. A buffer laid out any other way is *reinterpreted*
/// rather than refused, and every call still reports success:
///
/// - An `XRGB8888` image — the format everything else in this crate scans out — has its unused
///   byte read as alpha. That byte is zero, so the cursor is completely transparent, on the legacy
///   interface only.
/// - A driver rounds a dumb buffer's rows up for its own reasons, which [`DumbBuffer::stride`]
///   states. Rows longer than four bytes a pixel are read sheared.
///
/// Neither substitution is in a vendored header; both are the kernel's `drm_mode_cursor_universal`
/// transcribed here. So [`format`](CursorImage::format) and [`stride`](CursorImage::stride) travel
/// with the image, and the legacy path refuses one it would misread rather than showing the
/// result. The atomic path reads them from the framebuffer instead and takes any layout the plane
/// advertises.
///
/// [`DumbBuffer::handle`]: crate::buffer::DumbBuffer::handle
/// [`DumbBuffer::stride`]: crate::buffer::DumbBuffer::stride
#[derive(Debug, Clone, Copy)]
pub struct CursorImage {
    /// The framebuffer the atomic interface scans the image out of.
    ///
    /// `None` where the caller registered none. The atomic interface refuses that, because
    /// `FB_ID` is the only way it can name an image. A caller that drives a legacy device alone
    /// pays for no framebuffer: `DRM_IOCTL_MODE_CURSOR2` never reads one, and registering it costs
    /// a kernel object per image and an `ADDFB2` that fails where no plane advertises the format.
    pub framebuffer: Option<Framebuffer>,
    /// The GEM handle the legacy interface names the same buffer by.
    pub handle: u32,
    /// How wide the image is, in pixels.
    pub width: u32,
    /// How tall the image is, in pixels.
    pub height: u32,
    /// How many bytes one row of the buffer takes.
    ///
    /// The legacy interface reads this many bytes a row whatever it is told, so it is checked
    /// against [`CursorImage::legacy_stride`] there rather than sent.
    pub stride: u32,
    /// What the buffer holds.
    ///
    /// The legacy interface reads [`CursorImage::LEGACY_FORMAT`] whatever it is told, so this is
    /// checked against that there rather than sent.
    pub format: Format,
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

impl CursorImage {
    /// What the legacy interface reads a cursor buffer as.
    ///
    /// `drm_mode_cursor2` carries no format, and this is the one the kernel puts in its place.
    /// [`Commit::set_cursor`](crate::Commit::set_cursor) on the legacy interface refuses an image
    /// in any other, because the result of sending one is a cursor that is wrong on the screen and
    /// right in every return value.
    pub const LEGACY_FORMAT: Format = Format::ARGB8888;

    /// Returns how many bytes a row of a cursor `width` pixels wide takes on the legacy interface.
    ///
    /// Four bytes a pixel with no rounding, which is the stride the kernel puts in place of the
    /// one `drm_mode_cursor2` does not carry. Answered as a `u64`, so that the product of a width
    /// no buffer could have still fits.
    ///
    /// ```
    /// use zgui_drm::cursor::CursorImage;
    ///
    /// // A driver that rounded a 60-pixel row up to 256 bytes describes a buffer the legacy
    /// // interface reads sheared, because it reads 240 bytes a row whatever it is told.
    /// assert_eq!(CursorImage::legacy_stride(60), 240);
    /// assert_ne!(CursorImage::legacy_stride(60), 256);
    /// ```
    pub const fn legacy_stride(width: u32) -> u64 {
        width as u64 * 4
    }
}

/// Where a cursor goes: the CRTC that shows it, and the plane the atomic interface puts it on.
#[derive(Debug, Clone, Copy)]
pub struct CursorPlane {
    /// The CRTC showing the cursor.
    ///
    /// The legacy interface names this and reads nothing else about the target.
    pub crtc: u32,
    /// The plane id, as [`Device::cursor_plane`] answered it.
    ///
    /// `None` where the device offers no cursor plane for this CRTC. The atomic interface refuses
    /// that, because a plane id is the only way it can name a cursor at all. The legacy interface
    /// names the CRTC and never reads this.
    pub id: Option<u32>,
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

    /// Returns a cursor plane that can drive the CRTC at `crtc_index` and is not in `taken`.
    ///
    /// `crtc_index` is a place in [`Resources::crtcs`](crate::resources::Resources::crtcs),
    /// because a place is what
    /// [`Plane::possible_crtcs`](crate::resources::Plane::possible_crtcs) indexes.
    ///
    /// A plane is a cursor plane when its `type` property holds the kernel's cursor value, read
    /// here the way any other property is read.
    ///
    /// # The planes a caller already took
    ///
    /// A plane's mask may name several CRTCs, and a plane drives one of them at a time: putting a
    /// cursor on it sets its `CRTC_ID`, and the display it was on loses its cursor with nothing
    /// reported. So a caller driving several displays hands in the ids it already assigned, and
    /// the second display is answered `None` rather than being given a plane that would take the
    /// first one's cursor away. A caller driving one display passes an empty slice.
    ///
    /// # What `None` means
    ///
    /// The device offers no cursor plane this CRTC can have. On the atomic interface the caller
    /// then draws the pointer into the frame itself. Three things produce it:
    ///
    /// - The hardware has no cursor plane for that CRTC.
    /// - The device was opened for the legacy interface. The kernel hides primary and cursor
    ///   planes from a client that did not ask for universal planes, and the legacy cursor request
    ///   names the CRTC and needs no plane.
    /// - The driver is para-virtualised — vmwgfx, qxl, virtio, virtualbox — and this device is on
    ///   the atomic interface. Such a driver hides its cursor plane from an atomic client that has
    ///   not set `DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT`. This crate leaves that capability alone,
    ///   because it has no way to send the hotspot those drivers then require. So an atomic client
    ///   on a virtual machine has no hardware cursor, and this call reports that before a commit
    ///   can fail on it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`](crate::Error::Ioctl) when the kernel refuses a read, and
    /// [`Error::Unusable`](crate::Error::Unusable) when a count kept moving under one.
    pub fn cursor_plane(&self, crtc_index: usize, taken: &[u32]) -> Result<Option<u32>> {
        for id in self.planes()? {
            // Cheapest first: a claimed plane costs no ioctl, the mask costs two, and the
            // properties cost one for every property the plane has.
            if taken.contains(&id) || !self.plane(id)?.drives(crtc_index) {
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

        // And every plane the lookup answers with reports that value. The check is anchored to the
        // name the kernel put beside the number, so it needs no second copy of the number itself.
        let Ok(resources) = device.resources() else {
            eprintln!("this device does not enumerate, so the lookup was not checked");
            return;
        };
        for index in 0..resources.crtcs.len() {
            let Ok(Some(id)) = device.cursor_plane(index, &[]) else {
                continue;
            };
            assert_eq!(
                device
                    .properties(id, ObjectKind::Plane)
                    .ok()
                    .and_then(|properties| properties.value("type")),
                cursor,
                "plane {id} was picked as the cursor plane for the CRTC at place {index}"
            );
        }
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
