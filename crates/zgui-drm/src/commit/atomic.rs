//! The atomic interface: describe the whole configuration, apply it in one call.
//!
//! A commit is a list of `(object, property, value)`. The kernel reads it out of four parallel
//! arrays: the objects, how many properties each of them carries, the property ids flattened in
//! object order, and their values at the same indices. [`Request`] builds those four.
//!
//! # The property cache
//!
//! A property is named to the kernel by id, and an id is found by reading every property of the
//! object and comparing names. That costs one `MODE_GETPROPERTY` per property per object. The set
//! does not change while the device is open, so it is read once and kept. Reading it per frame
//! would put dozens of ioctls in front of every flip.
//!
//! # What a flip must name
//!
//! [`AtomicCommit::flip`] sets the plane's `FB_ID` and nothing else. It asks for
//! `DRM_MODE_PAGE_FLIP_EVENT`, and the header states the rule under that flag: a CRTC is in a
//! commit "if one of its properties is set, or if a property is set on a connector or plane linked
//! via the CRTC_ID property to the CRTC", and "at least one CRTC must be included". So a flip is
//! valid once [`AtomicCommit::modeset`] has linked the plane to the CRTC. A flip on a plane linked
//! to nothing puts no CRTC in the commit, which the header forbids.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::commit::{Commit, Pipe};
use crate::cursor::{CursorImage, CursorPlane};
use crate::device::Device;
use crate::error::{Error, Result};
use crate::framebuffer::Framebuffer;
use crate::ioctl;
use crate::property::{ObjectKind, Properties};
use crate::resources::Mode;
use crate::sys;

// `raw_bytes` reads every byte of a `drm_mode_modeinfo`, which is sound only where every byte
// belongs to a field. A `u32`, ten `u16`, three more `u32` and a 32-byte name sum to 68, and the
// structure is 68 bytes with an alignment of 4, so it has no interior and no trailing padding.
// This is the claim the safety comment on `raw_bytes` rests on, checked where it is made.
const _: () = assert!(
    size_of::<sys::drm_mode_modeinfo>() == 4 + 10 * 2 + 4 + 4 + 4 + 32,
    "drm_mode_modeinfo gained padding, so its bytes are no longer all initialised"
);

/// One commit under construction, in the four arrays the kernel reads it from.
#[derive(Debug, Default)]
struct Request {
    /// The object ids, one per [`Request::add`].
    objects: Vec<u32>,
    /// How many properties each object carries, at the same index as the object.
    counts: Vec<u32>,
    /// Every property id, flattened in object order.
    properties: Vec<u32>,
    /// Every value, at the same index as its property.
    values: Vec<u64>,
}

impl Request {
    /// Adds `properties` to the commit, as the properties of `object`.
    ///
    /// Every call site names a fixed list, so a length that a `u32` cannot hold is unreachable.
    fn add(&mut self, object: u32, properties: &[(u32, u64)]) {
        self.objects.push(object);
        self.counts.push(properties.len() as u32);
        for &(property, value) in properties {
            self.properties.push(property);
            self.values.push(value);
        }
    }

    /// Hands the commit to the kernel with `flags`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses it.
    fn issue(&mut self, device: &Device, flags: u32) -> Result<()> {
        let mut request = sys::drm_mode_atomic {
            flags,
            // One object per `add`, so this is bounded the same way the counts are.
            count_objs: self.objects.len() as u32,
            objs_ptr: self.objects.as_mut_ptr() as u64,
            // A pointer despite the name: it points at the per-object property counts.
            count_props_ptr: self.counts.as_mut_ptr() as u64,
            props_ptr: self.properties.as_mut_ptr() as u64,
            prop_values_ptr: self.values.as_mut_ptr() as u64,
            // `drm_mode_atomic_ioctl` refuses a commit whose `reserved` is not zero.
            reserved: 0,
            // Returned in the flip event. Zero: the event already names the CRTC, and that is how
            // a caller tells one pipe from another.
            user_data: 0,
        };
        ioctl::issue(device.fd(), ioctl::MODE_ATOMIC, &mut request)
    }
}

/// The atomic commit interface.
#[derive(Debug, Default)]
pub struct AtomicCommit {
    /// Each object's properties, by object id and kind.
    ///
    /// The kind is part of the key because the id spaces are shared: the same number can name a
    /// connector and a plane.
    cache: HashMap<(u32, u32), Properties>,
    /// The mode blob each CRTC is currently set from, by CRTC id.
    ///
    /// A modeset makes a new blob and leaves the one it replaced to be destroyed. Without this,
    /// every mode change would leave a blob in the kernel until the device is closed. The blob of
    /// the mode still on screen stays here, and closing the device releases it.
    modes: HashMap<u32, u32>,
}

impl AtomicCommit {
    /// Creates a commit interface with nothing read yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the properties of one object, reading them on the first ask and keeping them.
    ///
    /// # Errors
    ///
    /// Returns whatever [`Device::properties`] failed with.
    fn properties(
        &mut self,
        device: &Device,
        object: u32,
        kind: ObjectKind,
    ) -> Result<&Properties> {
        match self.cache.entry((object, kind.as_raw())) {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => Ok(entry.insert(device.properties(object, kind)?)),
        }
    }

    /// Turns each named property of `object` into its id, keeping the value beside it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] naming the object and the first property it does not have,
    /// which is how a device that cannot be driven atomically reports what it lacks.
    fn resolve(
        &mut self,
        device: &Device,
        object: u32,
        kind: ObjectKind,
        wanted: &[(&str, u64)],
    ) -> Result<Vec<(u32, u64)>> {
        wanted
            .iter()
            .map(|(name, value)| {
                let id = self
                    .properties(device, object, kind)?
                    .id(name)
                    .ok_or_else(|| {
                        Error::Unusable(format!(
                            "{kind:?} {object} has no {name} property, which an atomic commit \
                             needs"
                        ))
                    })?;
                Ok((id, *value))
            })
            .collect()
    }

    /// Returns the plane a cursor is committed to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when `plane` names no plane. The legacy interface addresses the
    /// CRTC and needs none; this one has no other way to name a cursor at all.
    fn cursor(plane: CursorPlane) -> Result<u32> {
        if plane.id == 0 {
            return Err(Error::Unusable(format!(
                "CRTC {} has no cursor plane, so an atomic commit cannot name a cursor on it",
                plane.crtc
            )));
        }
        Ok(plane.id)
    }

    /// Commits `properties` on the cursor plane `id`.
    ///
    /// The commit blocks and asks for no event. A cursor update on a live CRTC is a flip, and the
    /// kernel refuses a non-blocking commit with `EBUSY` while another one is outstanding on that
    /// CRTC — which a pointer moved per input event would meet against the frame loop's own flips.
    /// A blocking commit waits for that one instead, so it costs at most one refresh. Nothing
    /// waits on a cursor, so no event is asked for.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses.
    fn commit_cursor(device: &Device, id: u32, properties: &[(u32, u64)]) -> Result<()> {
        let mut request = Request::default();
        request.add(id, properties);
        request.issue(device, 0)
    }
}

impl Commit for AtomicCommit {
    fn can_test(&self) -> bool {
        true
    }

    fn modeset(
        &mut self,
        device: &Device,
        pipe: Pipe,
        mode: &Mode,
        framebuffer: Framebuffer,
    ) -> Result<()> {
        // The timings travel as a blob, and `MODE_ID` names the blob. The one this replaces is
        // destroyed further down, once the commit succeeded.
        let blob = device.create_blob(raw_bytes(&mode.raw))?;
        let width = mode.width();
        let height = mode.height();

        let crtc = self.resolve(
            device,
            pipe.crtc,
            ObjectKind::Crtc,
            &[("MODE_ID", u64::from(blob)), ("ACTIVE", 1)],
        )?;
        let connector = self.resolve(
            device,
            pipe.connector,
            ObjectKind::Connector,
            &[("CRTC_ID", u64::from(pipe.crtc))],
        )?;
        let plane = self.resolve(
            device,
            pipe.plane,
            ObjectKind::Plane,
            &[
                ("FB_ID", u64::from(framebuffer.id())),
                ("CRTC_ID", u64::from(pipe.crtc)),
                // The destination rectangle is in whole pixels.
                ("CRTC_X", 0),
                ("CRTC_Y", 0),
                ("CRTC_W", u64::from(width)),
                ("CRTC_H", u64::from(height)),
                // The source rectangle is in 16.16 fixed point.
                ("SRC_X", 0),
                ("SRC_Y", 0),
                ("SRC_W", fixed_16_16(width)),
                ("SRC_H", fixed_16_16(height)),
            ],
        )?;

        let mut request = Request::default();
        request.add(pipe.crtc, &crtc);
        request.add(pipe.connector, &connector);
        request.add(pipe.plane, &plane);

        // The dry run first. A modeset the hardware cannot do is refused here and leaves the
        // display as it was, which is the capability the legacy path lacks. `ALLOW_MODESET` is set
        // on both calls, so the two ask the same question: `drm_atomic_check_only` refuses a
        // commit that needs a full modeset while the flag is off.
        //
        // `drm_mode_atomic_ioctl` answers `EINVAL` for a test-only commit that asks for an event,
        // so neither call carries one. A caller learns the modeset finished when the second call
        // returns.
        let applied = request
            .issue(
                device,
                sys::DRM_MODE_ATOMIC_TEST_ONLY | sys::DRM_MODE_ATOMIC_ALLOW_MODESET,
            )
            .and_then(|()| request.issue(device, sys::DRM_MODE_ATOMIC_ALLOW_MODESET));

        // A release can fail only where this crate lost track of a blob it made, and it changes
        // nothing about what the display is doing. So both of the releases below report the
        // outcome of the commit and drop their own: a caller told that a modeset failed would undo
        // one that worked.
        if let Err(error) = applied {
            // The new mode never reached the screen, so its blob describes nothing.
            drop(device.destroy_blob(blob));
            return Err(error);
        }

        // The old blob is released only here, after the apply. A commit that failed leaves the
        // previous mode on screen, and the previous blob describes it, so a release before the
        // apply would throw away the description of what the display still shows. The header
        // allows the release now: a blob may go "as soon as the commit has been issued, without
        // waiting for it to complete". The kernel took its own reference on the blob when the
        // commit set `MODE_ID`, and it holds that for as long as the mode is live.
        if let Some(spent) = self.modes.insert(pipe.crtc, blob) {
            drop(device.destroy_blob(spent));
        }

        Ok(())
    }

    fn flip(&mut self, device: &Device, pipe: Pipe, framebuffer: Framebuffer) -> Result<()> {
        let plane = self.resolve(
            device,
            pipe.plane,
            ObjectKind::Plane,
            &[("FB_ID", u64::from(framebuffer.id()))],
        )?;

        let mut request = Request::default();
        request.add(pipe.plane, &plane);
        // Without the event flag nothing ever tells a frame loop the old buffer is free again.
        request.issue(
            device,
            sys::DRM_MODE_ATOMIC_NONBLOCK | sys::DRM_MODE_PAGE_FLIP_EVENT,
        )
    }

    fn set_cursor(
        &mut self,
        device: &Device,
        plane: CursorPlane,
        image: CursorImage,
        x: i32,
        y: i32,
    ) -> Result<()> {
        let id = Self::cursor(plane)?;
        // The hotspot is dropped here. Only `DRM_IOCTL_MODE_CURSOR2` has a field for it, and the
        // property set has no standard equivalent, so what reaches the kernel is the position
        // alone. `CursorImage::hotspot_x` states what a caller does about that.
        let properties = self.resolve(
            device,
            id,
            ObjectKind::Plane,
            &[
                ("FB_ID", u64::from(image.framebuffer.id())),
                ("CRTC_ID", u64::from(plane.crtc)),
                // The destination rectangle is in whole pixels, and its position is signed.
                ("CRTC_X", signed(x)),
                ("CRTC_Y", signed(y)),
                // The same extent as the source, so the display engine scales nothing.
                ("CRTC_W", u64::from(image.width)),
                ("CRTC_H", u64::from(image.height)),
                // The source rectangle is in 16.16 fixed point, and covers the whole image.
                ("SRC_X", 0),
                ("SRC_Y", 0),
                ("SRC_W", fixed_16_16(image.width)),
                ("SRC_H", fixed_16_16(image.height)),
            ],
        )?;
        Self::commit_cursor(device, id, &properties)
    }

    fn move_cursor(&mut self, device: &Device, plane: CursorPlane, x: i32, y: i32) -> Result<()> {
        let id = Self::cursor(plane)?;
        // Only the position. The plane keeps the framebuffer and the rectangles `set_cursor` gave
        // it, and the CRTC is in the commit because `CRTC_ID` still links the plane to it — the
        // same rule the head of this module states for a flip.
        let properties = self.resolve(
            device,
            id,
            ObjectKind::Plane,
            &[("CRTC_X", signed(x)), ("CRTC_Y", signed(y))],
        )?;
        Self::commit_cursor(device, id, &properties)
    }

    fn hide_cursor(&mut self, device: &Device, plane: CursorPlane) -> Result<()> {
        let id = Self::cursor(plane)?;
        // A plane is turned off by clearing both together. The CRTC it was linked to is pulled
        // into the commit by the link this takes away.
        let properties = self.resolve(
            device,
            id,
            ObjectKind::Plane,
            &[("FB_ID", 0), ("CRTC_ID", 0)],
        )?;
        Self::commit_cursor(device, id, &properties)
    }
}

/// Returns `pixels` in the 16.16 fixed point a plane's source rectangle is stated in.
///
/// `SRC_X`, `SRC_Y`, `SRC_W` and `SRC_H` are the one part of a plane commit that is not in whole
/// pixels: a whole pixel is `1 << 16`. A rectangle handed over in pixels asks for a source
/// 1/65536th of the size, which the driver either refuses or scales into nothing.
const fn fixed_16_16(pixels: u32) -> u64 {
    (pixels as u64) << 16
}

/// Returns `pixels` as the value a signed range property takes.
///
/// Every value in a commit travels as a `u64`, and the kernel reads a signed range property back
/// by casting the whole 64 bits. So a negative coordinate has to be sign-extended to 64 bits; a
/// narrower conversion puts the object several thousand million pixels to the right.
///
/// A cursor reaches this in ordinary use. Its position is its top left corner, so a pointer near
/// the left or the top edge of a display puts the image at a negative coordinate.
const fn signed(pixels: i32) -> u64 {
    pixels as i64 as u64
}

/// Returns the bytes of `mode`, as the kernel's own structure.
///
/// These go into the `MODE_ID` blob. The kernel copies them back out as a `drm_mode_modeinfo`, so
/// they travel as the bytes they already are.
fn raw_bytes(mode: &sys::drm_mode_modeinfo) -> &[u8] {
    // SAFETY: `drm_mode_modeinfo` is `#[repr(C)]` and every field is an unsigned integer or an
    // array of `c_char`, so every byte of it is initialised and every bit pattern of it is a
    // value of it. The structure has no padding — the assertion at the head of this module checks
    // that its fields sum to its size — so no byte in the range read here is uninitialised. The
    // pointer comes from a live reference, so it is non-null and aligned for `u8`, and the slice
    // borrows `mode` for exactly as long as it lives.
    unsafe {
        std::slice::from_raw_parts(
            std::ptr::from_ref(mode).cast::<u8>(),
            size_of::<sys::drm_mode_modeinfo>(),
        )
    }
}

#[cfg(test)]
mod tests {
    //! The shape of the arrays the kernel reads a commit out of.
    //!
    //! The four are parallel in two different ways at once: `counts` runs alongside `objects`, and
    //! `properties` runs alongside `values` while being partitioned by `counts`. The kernel reads
    //! exactly `counts[i]` ids per object out of the flattened arrays, and the compiler accepts a
    //! count that is wrong either way: an overcount reads past what the object owns and is refused
    //! with an errno that names none of it, and an undercount is applied as the shorter commit it
    //! describes.

    use super::*;

    #[test]
    fn a_commit_holds_one_count_per_object_and_the_properties_in_object_order() {
        let mut request = Request::default();
        request.add(31, &[(7, 100), (9, 200)]);
        request.add(42, &[(11, 300)]);

        assert_eq!(
            request.objects,
            [31, 42],
            "one entry per object, in the order they were added"
        );
        assert_eq!(
            request.counts,
            [2, 1],
            "one count per object, saying how many of the properties are its own"
        );
        assert_eq!(
            request.properties,
            [7, 9, 11],
            "every property id, flattened in object order"
        );
        assert_eq!(
            request.values,
            [100, 200, 300],
            "every value, at the same index as its property"
        );
    }

    #[test]
    fn a_whole_pixel_of_a_source_rectangle_is_one_shifted_by_sixteen() {
        assert_eq!(fixed_16_16(0), 0);
        assert_eq!(fixed_16_16(1), 65_536, "one pixel is 1 << 16");
        assert_eq!(fixed_16_16(64), 4_194_304, "a 64-pixel cursor is 64 << 16");
        assert_eq!(
            fixed_16_16(1920),
            125_829_120,
            "a whole display's width still fits, so nothing has to be clamped"
        );
        // The widest extent the kernel can describe is still a `u64` after the shift, so the
        // conversion never wraps.
        assert_eq!(fixed_16_16(u32::MAX), 0x0000_ffff_ffff_0000);
    }

    #[test]
    fn a_negative_coordinate_reaches_the_kernel_as_the_whole_sixty_four_bits() {
        assert_eq!(signed(0), 0);
        assert_eq!(signed(1), 1);
        // The kernel casts the whole `u64` back to a signed value, so -1 is every bit set. A
        // conversion that only widened the 32 bits would send 4294967295, which is a cursor about
        // four thousand million pixels off the right of the display.
        assert_eq!(signed(-1), u64::MAX);
        assert_eq!(signed(-64), u64::MAX - 63);
        assert_eq!(signed(i32::MIN), 0xffff_ffff_8000_0000);
        assert_eq!(signed(i32::MAX), 0x7fff_ffff);
    }

    #[test]
    fn a_mode_is_as_many_bytes_as_the_kernel_reads_a_mode_from() {
        let mode = sys::drm_mode_modeinfo {
            clock: 148_500,
            hdisplay: 1920,
            vdisplay: 1080,
            ..Default::default()
        };
        let bytes = raw_bytes(&mode);

        assert_eq!(
            bytes.len(),
            68,
            "a mode blob is the whole structure the header declares"
        );
        // The first field is the pixel clock. Reading it back pins the bytes to this structure and
        // to its field order.
        assert_eq!(
            u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            148_500,
            "the bytes are the structure's own, in its own order"
        );
    }
}
