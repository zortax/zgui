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
//! [`AtomicCommit::flip`] sets the plane's `FB_ID`, and its `IN_FENCE_FD` where the caller handed
//! one over. It asks for `DRM_MODE_PAGE_FLIP_EVENT`, and the header states the rule under that
//! flag: a CRTC is in a commit "if one of its properties is set, or if a property is set on a
//! connector or plane linked via the CRTC_ID property to the CRTC", and "at least one CRTC must be
//! included". So a flip is valid once [`AtomicCommit::modeset`] has linked the plane to the CRTC.
//! A flip on a plane linked to nothing puts no CRTC in the commit, which that rule rules out.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::os::fd::{AsRawFd, BorrowedFd};

use crate::commit::{Commit, Pipe, legacy};
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

    /// Empties the commit, keeping the space its four arrays already hold.
    ///
    /// One request is reused this way. A cursor moves once per input event, and that path would
    /// otherwise allocate four fresh vectors for two property values every time.
    fn clear(&mut self) {
        self.objects.clear();
        self.counts.clear();
        self.properties.clear();
        self.values.clear();
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
    /// Each cursor plane's `CRTC_X` and `CRTC_Y` property ids, by plane id.
    ///
    /// [`AtomicCommit::move_cursor`] runs once per pointer motion. Resolving two names against a
    /// `HashMap<String, _>` and collecting the answers into a vector every time is the only cost
    /// that path has, and this removes it.
    positions: HashMap<u32, (u32, u32)>,
    /// The arrays a cursor commit is built in, kept between commits.
    ///
    /// A cursor commit names one object and at most ten properties, and it is issued as often as a
    /// pointer moves. Reusing the four vectors makes a move allocate nothing.
    scratch: Request,
    /// The CRTCs whose cursor the legacy request refused to move.
    ///
    /// [`AtomicCommit::move_cursor`] tries the cheap request first and falls back to a property
    /// commit when the kernel refuses it. A CRTC the driver registered no cursor plane on answers
    /// every such request the same way, so the refusal is a fact about the CRTC. It is recorded
    /// here and the cheap request is asked for once.
    slow_moves: HashSet<u32>,
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

    /// Returns the id of one named property of `object`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] naming the object and the property it does not have, which is
    /// how a device that cannot be driven atomically reports what it lacks.
    fn property(
        &mut self,
        device: &Device,
        object: u32,
        kind: ObjectKind,
        name: &str,
    ) -> Result<u32> {
        self.properties(device, object, kind)?
            .id(name)
            .ok_or_else(|| {
                Error::Unusable(format!(
                    "{kind:?} {object} has no {name} property, which an atomic commit needs"
                ))
            })
    }

    /// Turns each named property of `object` into its id, keeping the value beside it.
    ///
    /// # Errors
    ///
    /// Returns whatever [`AtomicCommit::property`] failed with for the first name that is absent.
    fn resolve(
        &mut self,
        device: &Device,
        object: u32,
        kind: ObjectKind,
        wanted: &[(&str, u64)],
    ) -> Result<Vec<(u32, u64)>> {
        wanted
            .iter()
            .map(|(name, value)| Ok((self.property(device, object, kind, name)?, *value)))
            .collect()
    }

    /// Returns the plane a cursor is committed to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when `plane` names no plane. The legacy interface addresses the
    /// CRTC and needs none; this one has no other way to name a cursor at all.
    fn cursor(plane: CursorPlane) -> Result<u32> {
        plane.id.ok_or_else(|| {
            Error::Unusable(format!(
                "CRTC {} has no cursor plane, so an atomic commit cannot name a cursor on it",
                plane.crtc
            ))
        })
    }

    /// Returns the `CRTC_X` and `CRTC_Y` property ids of a cursor plane, reading them once and
    /// keeping them.
    ///
    /// # Errors
    ///
    /// Returns whatever [`AtomicCommit::property`] failed with.
    fn position(&mut self, device: &Device, plane: u32) -> Result<(u32, u32)> {
        if let Some(ids) = self.positions.get(&plane) {
            return Ok(*ids);
        }
        let ids = (
            self.property(device, plane, ObjectKind::Plane, "CRTC_X")?,
            self.property(device, plane, ObjectKind::Plane, "CRTC_Y")?,
        );
        self.positions.insert(plane, ids);
        Ok(ids)
    }

    /// Puts `properties` in the scratch commit as the properties of the plane `id`.
    ///
    /// The caller issues it, so that the flags a cursor commit carries stay visible where the
    /// commit is written.
    fn stage(&mut self, id: u32, properties: &[(u32, u64)]) {
        self.scratch.clear();
        self.scratch.add(id, properties);
    }

    /// Returns `true` while a cursor on `crtc` is still moved through the legacy request.
    ///
    /// True until that CRTC refuses one. [`AtomicCommit::move_cursor`] states what the two answers
    /// cost and why the refusal is remembered per CRTC.
    fn moves_quickly(&self, crtc: u32) -> bool {
        !self.slow_moves.contains(&crtc)
    }

    /// Records that `crtc` refused the legacy request, so it is asked once.
    fn refused_a_quick_move(&mut self, crtc: u32) {
        self.slow_moves.insert(crtc);
    }
}

/// The flags every cursor property commit carries: it blocks, and it asks for no event.
///
/// `stall_checks` refuses a non-blocking commit with `EBUSY` while the previous commit on that
/// CRTC has not completed, and a pointer moved per input event would meet the frame loop's own
/// flips constantly. A blocking commit waits for the outstanding one instead. Nothing waits on a
/// cursor, so no event is asked for.
///
/// The waiting costs two vertical blanks. `commit_tail` waits for the outstanding commit in
/// `drm_atomic_helper_wait_for_dependencies`, then `drm_atomic_helper_commit_tail` waits for the
/// next blank in `drm_atomic_helper_wait_for_vblanks`. About 33 ms at 60 Hz.
///
/// So a motion does not use this. [`AtomicCommit::move_cursor`] issues the legacy request, which
/// the kernel gives a shortcut past both waits. What is left on this path is putting an image on
/// the plane and taking it off, which a caller reaches when the cursor's shape changes.
const CURSOR_COMMIT: u32 = 0;

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
        fence: Option<BorrowedFd<'_>>,
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
        let mut wanted = vec![
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
        ];
        // The first frame of a display arrives through the modeset, so this carries a fence for
        // the same reason a flip does. The dry run below names it too. `sync_file_get_fence` takes
        // a reference to the fence inside the file and leaves the file open, so the same
        // descriptor goes across twice and is still the caller's afterwards.
        if let Some(fence) = fence {
            wanted.push(("IN_FENCE_FD", fenced(fence)));
        }
        let plane = self.resolve(device, pipe.plane, ObjectKind::Plane, &wanted)?;

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

    fn flip(
        &mut self,
        device: &Device,
        pipe: Pipe,
        framebuffer: Framebuffer,
        fence: Option<BorrowedFd<'_>>,
    ) -> Result<()> {
        let mut wanted = vec![("FB_ID", u64::from(framebuffer.id()))];
        // The fast path a caller drawing with a graphics device takes: the frame is committed while
        // the device is still finishing it, and the kernel holds the flip back until this fence
        // signals.
        if let Some(fence) = fence {
            wanted.push(("IN_FENCE_FD", fenced(fence)));
        }
        let plane = self.resolve(device, pipe.plane, ObjectKind::Plane, &wanted)?;

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
        let framebuffer = image.framebuffer.ok_or_else(|| {
            Error::Unusable(
                "a cursor image for an atomic commit needs a framebuffer, because FB_ID is the \
                 only way a plane names one"
                    .to_owned(),
            )
        })?;

        // The hotspot is dropped here. Only `DRM_IOCTL_MODE_CURSOR2` has a field for it, and the
        // property set has no standard equivalent, so what reaches the kernel is the position
        // alone. `CursorImage::hotspot_x` states what a caller does about that.
        let properties = self.resolve(
            device,
            id,
            ObjectKind::Plane,
            &[
                ("FB_ID", u64::from(framebuffer.id())),
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

        self.stage(id, &properties);
        // The dry run this interface has and the legacy one lacks. This commit is the one that
        // turns the cursor plane on, which is the configuration a driver is most likely to refuse
        // — a format the plane will not take, an extent past what its hardware allows — and a
        // refusal here leaves the display exactly as it was. A move skips the test, because it
        // would double what every pointer motion costs.
        //
        // `ALLOW_MODESET` stays off. A cursor on a CRTC that is already running needs no modeset,
        // and `drm_atomic_check_only` answers `EINVAL` for a commit that needs one while the flag
        // is off. So a driver that wants a full modeset here refuses the update, and the caller
        // reads the error instead of the display going down inside a pointer update.
        self.scratch.issue(device, sys::DRM_MODE_ATOMIC_TEST_ONLY)?;
        self.scratch.issue(device, CURSOR_COMMIT)
    }

    /// Moves the cursor through `DRM_IOCTL_MODE_CURSOR2`, falling back to a property commit on a
    /// CRTC that refuses one.
    ///
    /// # What the legacy request skips
    ///
    /// Both requests end in the same place. `drm_mode_cursor_universal` builds a plane update and
    /// hands it to `drm_atomic_helper_update_plane`, so on an atomic driver the legacy request is
    /// an atomic commit. They differ in one flag the legacy path can set and the atomic ioctl
    /// cannot: `drm_atomic_helper_update_plane` sets `legacy_cursor_update` when the plane it is
    /// given is the CRTC's own cursor plane, and `drm_mode_atomic_ioctl` sets it nowhere. Three
    /// things follow from that flag, and each of them is a wait this path avoids:
    ///
    /// * `drm_atomic_helper_wait_for_vblanks` returns at once;
    /// * `drm_atomic_helper_setup_commit` completes the CRTC's `flip_done` and records no commit
    ///   in the state, so `drm_atomic_helper_wait_for_flip_done` skips that CRTC;
    /// * `drm_atomic_helper_check` may promote the whole commit to `async_update`, where a driver
    ///   with `atomic_async_check` and `atomic_async_update` programs the plane and returns.
    ///
    /// One wait survives. `drm_atomic_helper_wait_for_dependencies` reads no such flag, so a move
    /// issued while a flip is outstanding on the same CRTC can still wait for that flip: one
    /// vertical blank instead of two, and none at all on a driver that took the asynchronous path.
    /// That is what this saves, and it is why a caller that owns a frame loop moves the cursor
    /// once a turn.
    ///
    /// # Mixing the two interfaces
    ///
    /// The kernel's documentation of the plane `type` property, which lives in `drm_plane.c` and
    /// in no vendored header, says a client must not drive a cursor plane through atomic commits
    /// and through these ioctls at once. The reason it gives is that the kernel uses some cursor
    /// planes implicitly in those ioctls. The divergence this crate can find is `crtc->cursor_x`
    /// and `crtc->cursor_y`: the legacy path keeps them and an atomic commit does not write them.
    /// A `DRM_MODE_CURSOR_BO` request that carries no `DRM_MODE_CURSOR_MOVE` reads them, so it
    /// would put the image back where the last legacy request left it.
    ///
    /// Nothing here sends that request. [`AtomicCommit::set_cursor`] states the position as
    /// `CRTC_X` and `CRTC_Y` in its own commit, and this request always carries `CURSOR_MOVE`,
    /// which is the flag `drm_mode_cursor_universal` writes both shadow fields under. So the
    /// sequence this crate issues has no reader of a field the other interface left stale.
    ///
    /// # The plane this cannot compare
    ///
    /// `drm_mode_cursor_universal` acts on `crtc->cursor`, the plane the driver registered as that
    /// CRTC's cursor. [`CursorPlane::id`] is the plane this crate found by reading plane types. On
    /// every driver this crate has met the two are one plane. Where they are not, the move reaches
    /// the driver's plane, the image stays on the other one, and the ioctl reports success: a
    /// cursor that stops moving with nothing logged. A refusal is caught below; this is not, and
    /// no property names `crtc->cursor` to compare against.
    fn move_cursor(&mut self, device: &Device, plane: CursorPlane, x: i32, y: i32) -> Result<()> {
        let id = Self::cursor(plane)?;
        if self.moves_quickly(plane.crtc) {
            match legacy::moved(device, plane.crtc, x, y) {
                Ok(()) => return Ok(()),
                // A CRTC the driver registered no cursor plane on answers `EFAULT`, and it answers
                // it for every later request as well. So the property commit takes over for that
                // CRTC and the display keeps its cursor.
                Err(_) => self.refused_a_quick_move(plane.crtc),
            }
        }
        // Only the position. The plane keeps the framebuffer and the rectangles `set_cursor` gave
        // it, and the CRTC is in the commit because `CRTC_ID` still links the plane to it — the
        // same rule the head of this module states for a flip. This commit asks for no event, so
        // the header's "at least one CRTC" rule does not apply to it: a plane that no `set_cursor`
        // linked has no `CRTC_ID`, and the kernel accepts the commit having done nothing.
        // `Commit::move_cursor` states that precondition.
        //
        // The two ids are cached and the request is reused, so this allocates nothing.
        let (crtc_x, crtc_y) = self.position(device, id)?;
        self.stage(id, &[(crtc_x, signed(x)), (crtc_y, signed(y))]);
        self.scratch.issue(device, CURSOR_COMMIT)
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
        self.stage(id, &properties);
        self.scratch.issue(device, CURSOR_COMMIT)
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

/// Returns `fence` as the value the `IN_FENCE_FD` property takes.
///
/// `drm_mode_config.c` creates the property as a signed range from -1 to `INT_MAX`, where -1 is
/// "no fence", so it travels the way a signed coordinate does: sign-extended to the whole 64 bits
/// the kernel reads back.
///
/// Nothing here ever sends -1. A commit that carries no fence names the property not at all, and
/// the kernel clears a plane's fence while it duplicates the state for the next commit, so a frame
/// that asked for no wait does not inherit the last frame's.
fn fenced(fence: BorrowedFd<'_>) -> u64 {
    signed(fence.as_raw_fd())
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

    use std::os::fd::AsFd;

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
    fn a_fence_reaches_the_kernel_as_the_descriptor_number_it_is() {
        // The property is a signed range, so the number is sign-extended the way a coordinate is.
        // Every descriptor a process can hold is positive, so the number the kernel reads back is
        // the number that was sent.
        let stdout = std::io::stdout();
        let fence = stdout.as_fd();
        assert_eq!(fenced(fence), fence.as_raw_fd() as u64);
        assert!(
            fenced(fence) <= u64::from(u32::MAX),
            "a descriptor number is positive, so nothing is sign-extended in practice"
        );
    }

    #[test]
    fn a_crtc_that_refused_the_cheap_cursor_move_is_never_asked_for_one_again() {
        // The fallback is per CRTC and it is one way. A CRTC the driver registered no cursor plane
        // on refuses every one of these requests, so a commit interface that asked again would
        // issue one failing ioctl per pointer motion for the rest of the program — and it would
        // pay for the property commit on top of it.
        let mut commit = AtomicCommit::new();
        assert!(commit.moves_quickly(31));
        assert!(commit.moves_quickly(42));

        commit.refused_a_quick_move(31);

        assert!(!commit.moves_quickly(31));
        assert!(
            commit.moves_quickly(42),
            "one CRTC refusing says nothing about the next, so the second display keeps the cheap \
             request"
        );
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
            "a mode blob is every byte of the structure the header declares"
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
