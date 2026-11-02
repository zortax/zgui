//! Allocating a buffer that a display scans out of and a graphics device draws into.
//!
//! The crate names no zgui crate and is usable on its own. It is written to be named by
//! `zgui-platform-drm` and `zgui-scanout`, one step along the same layer, and by nothing else.
//!
//! The kernel's dumb buffers are the obvious thing to reach for and they are the wrong thing. A
//! dumb buffer is *CPU-writable and scanout-capable*, which is not the same as renderable: a
//! graphics driver handed one as a render target puts it where its own engine wants it, another
//! device pins it somewhere else the moment it imports the descriptor, and the submission that
//! would draw into it is then refused — on nouveau, `kernel rejected pushbuf: Invalid argument`,
//! several calls after the point where anything could have reported the mistake.
//!
//! `gbm_bo_create` states both requirements where the driver can act on them, which is at
//! allocation. It is what every Wayland compositor allocates scanout buffers with.
//!
//! # Which layout, and why one card is different from two
//!
//! There are two ways to allocate: naming the layouts that are acceptable, or letting the driver
//! choose. The kernel's buffer-exchange documentation is firm that a chain must not mix them —
//! *"the complete chain of operations formed by the producer and all the consumers must be either
//! fully implicit or fully explicit"* — and [`Layout`] is which form one buffer takes. Every
//! consumer of a buffer is told what [`Allocation::modifier`] answers, so the chain stays whole
//! whichever form it is in.
//!
//! **An implicit layout is a layout only one driver knows.** It travels with the GEM object rather
//! than with the descriptor, so the allocator's own display engine scans the buffer out correctly
//! while a *second* driver that imports the descriptor is told nothing and reads it as linear. On
//! one card that is invisible, because there is no second driver. On two it puts a tiled buffer
//! under a linear renderer, and the picture comes out as a regular, patterned scramble of itself.
//! [`Layout::Linear`] is what a caller drawing across a device boundary asks for.
//!
//! # Loaded, never linked
//!
//! For the reason every other library here is: a console session has to start on a machine that
//! has none of them. A build needs neither the library nor its headers, and a display whose
//! machine has no `libgbm` falls back to copying each frame.

// Every item here calls into libgbm over a file descriptor, and `std::os::fd` exists only on a
// Unix host, so on any other platform this crate is empty and `cargo check --workspace` passes on
// a machine this code could never run on. It is gated whole rather than item by item, as
// `zgui-drm` and `zgui-evdev` are, because those are split into modules and this is one file.
#![cfg(target_os = "linux")]
#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: libgbm is opened at run time and
// every call into it is a call through a pointer resolved here. A build that linked it would stop a
// console session starting on a machine that has no such library, which is the case the copied
// scanout path exists to answer.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::sync::Arc;

/// The buffer is written by the graphics device rather than by the processor.
const USE_RENDERING: u32 = 4;
/// The buffer is scanned out by a display engine.
const USE_SCANOUT: u32 = 1;
/// The buffer is laid out row after row, with no tiling.
const USE_LINEAR: u32 = 16;

/// The layout one buffer is allocated in.
///
/// See the head of this crate for why the answer is not the same on a machine with one graphics
/// card as on a machine with two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Whatever the driver prefers, which it names to nobody.
    ///
    /// The fastest layout the allocator's own hardware has, and the right ask where the device
    /// that allocates is the device that draws. What it costs is that the layout reaches no other
    /// driver: it is a property of the GEM object rather than of the descriptor exported from it.
    Driver,
    /// Linear, asked for.
    ///
    /// The one layout every driver agrees on, and what a buffer allocated on one card and drawn
    /// into by another has to be in. It is slower to scan out than a tiled layout on hardware that
    /// has one, and a picture that is correct is worth more than that.
    Linear,
}

impl Layout {
    /// The `gbm_bo_create` flag this layout adds.
    ///
    /// Advice rather than a requirement — see [`Device::allocate`], which is why the list below
    /// exists as well.
    const fn flag(self) -> u32 {
        match self {
            Self::Driver => 0,
            Self::Linear => USE_LINEAR,
        }
    }

    /// The layouts this asks for by name, where it names any.
    ///
    /// Nothing for [`Layout::Driver`]: naming every layout a driver has and naming none are
    /// different asks, and the second is the one that means "whatever suits you".
    const fn modifiers(self) -> Option<&'static [u64]> {
        match self {
            Self::Driver => None,
            Self::Linear => Some(&[LINEAR]),
        }
    }
}

/// `DRM_FORMAT_MOD_LINEAR`, the layout with no tiling in it.
pub const LINEAR: u64 = 0;

/// The code a driver answers when a buffer is in no layout it has a name for.
///
/// `DRM_FORMAT_MOD_INVALID`. It means the layout is implicit — agreed between the two ends by
/// something other than this number — and it is what the driver this was written against answers
/// for every buffer it makes.
pub const IMPLICIT: u64 = 0x00ff_ffff_ffff_ffff;

/// `libgbm`, loaded.
///
/// One per process is plenty; a [`Device`] holds a handle to it so that the library outlives every
/// buffer made through it.
#[derive(Debug)]
pub struct Library {
    /// Kept so the symbols below stay mapped.
    #[expect(
        dead_code,
        reason = "holds the mapping the function pointers point into"
    )]
    library: libloading::Library,
    create_device: unsafe extern "C" fn(c_int) -> *mut c_void,
    device_destroy: unsafe extern "C" fn(*mut c_void),
    backend_name: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    bo_create: unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void,
    /// `gbm_bo_create_with_modifiers2`, where this libgbm has it.
    ///
    /// Nothing before Mesa 21.1 does, and the form without the uses is all that older ones offer.
    /// A missing one is the ordinary answer rather than a failure to load, because the flagless
    /// form below and then [`Library::bo_create`] answer the same question less exactly.
    bo_create_with_modifiers2: Option<
        unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const u64, c_uint, u32) -> *mut c_void,
    >,
    /// `gbm_bo_create_with_modifiers`, where this libgbm has it.
    ///
    /// It states no uses. gbm reads the layout list as the whole of the requirement and allocates
    /// something both scanned out of and drawn into anyway, which is what every caller of it wants.
    bo_create_with_modifiers:
        Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const u64, c_uint) -> *mut c_void>,
    bo_destroy: unsafe extern "C" fn(*mut c_void),
    bo_get_fd: unsafe extern "C" fn(*mut c_void) -> c_int,
    bo_get_stride: unsafe extern "C" fn(*mut c_void) -> u32,
    bo_get_offset: unsafe extern "C" fn(*mut c_void, c_int) -> u32,
    bo_get_modifier: unsafe extern "C" fn(*mut c_void) -> u64,
    bo_get_plane_count: unsafe extern "C" fn(*mut c_void) -> c_int,
    bo_map: unsafe extern "C" fn(
        *mut c_void,
        u32,
        u32,
        u32,
        u32,
        u32,
        *mut u32,
        *mut *mut c_void,
    ) -> *mut c_void,
    bo_unmap: unsafe extern "C" fn(*mut c_void, *mut c_void),
}

/// `GBM_BO_TRANSFER_READ`.
const TRANSFER_READ: u32 = 1;

impl Library {
    /// Returns whether this libgbm can be asked for a buffer by naming the layouts.
    ///
    /// A libgbm that cannot leaves every buffer implicit, whatever a caller asks for: the flags
    /// are the only lever left and `GBM_BO_USE_LINEAR` is advice a driver may ignore. Worth
    /// reporting rather than inferring, because a refusal and an absent entry point produce the
    /// same buffer and want different answers.
    pub const fn names_layouts(&self) -> bool {
        self.bo_create_with_modifiers2.is_some() || self.bo_create_with_modifiers.is_some()
    }

    /// Loads it, or says why it could not.
    ///
    /// # Errors
    ///
    /// Returns the loader's own message where the library is absent, and the name of the first
    /// symbol that is missing where it is present but too old.
    pub fn load() -> Result<Arc<Self>, String> {
        // SAFETY: every name is libgbm's own and every signature is the one in `gbm.h`. Each
        // symbol is fetched *as its function type*: fetching one as a pointer would read the first
        // bytes of the function's code and call whatever those happened to spell.
        unsafe {
            let library =
                libloading::Library::new("libgbm.so.1").map_err(|error| error.to_string())?;
            macro_rules! symbol {
                ($name:literal, $kind:ty) => {{
                    let found: libloading::Symbol<'_, $kind> =
                        library.get($name).map_err(|_| {
                            format!(
                                "libgbm has no {}",
                                CStr::from_bytes_with_nul_unchecked($name).to_string_lossy()
                            )
                        })?;
                    *found
                }};
            }
            /// The same, for a name a libgbm is allowed not to have.
            macro_rules! optional {
                ($name:literal, $kind:ty) => {{
                    let found: Option<libloading::Symbol<'_, $kind>> = library.get($name).ok();
                    found.map(|found| *found)
                }};
            }
            Ok(Arc::new(Self {
                create_device: symbol!(
                    b"gbm_create_device\0",
                    unsafe extern "C" fn(c_int) -> *mut c_void
                ),
                device_destroy: symbol!(b"gbm_device_destroy\0", unsafe extern "C" fn(*mut c_void)),
                backend_name: symbol!(
                    b"gbm_device_get_backend_name\0",
                    unsafe extern "C" fn(*mut c_void) -> *const c_char
                ),
                bo_create: symbol!(
                    b"gbm_bo_create\0",
                    unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void
                ),
                bo_create_with_modifiers2: optional!(
                    b"gbm_bo_create_with_modifiers2\0",
                    unsafe extern "C" fn(
                        *mut c_void,
                        u32,
                        u32,
                        u32,
                        *const u64,
                        c_uint,
                        u32,
                    ) -> *mut c_void
                ),
                bo_create_with_modifiers: optional!(
                    b"gbm_bo_create_with_modifiers\0",
                    unsafe extern "C" fn(
                        *mut c_void,
                        u32,
                        u32,
                        u32,
                        *const u64,
                        c_uint,
                    ) -> *mut c_void
                ),
                bo_destroy: symbol!(b"gbm_bo_destroy\0", unsafe extern "C" fn(*mut c_void)),
                bo_get_fd: symbol!(
                    b"gbm_bo_get_fd\0",
                    unsafe extern "C" fn(*mut c_void) -> c_int
                ),
                bo_get_stride: symbol!(
                    b"gbm_bo_get_stride\0",
                    unsafe extern "C" fn(*mut c_void) -> u32
                ),
                bo_get_offset: symbol!(
                    b"gbm_bo_get_offset\0",
                    unsafe extern "C" fn(*mut c_void, c_int) -> u32
                ),
                bo_get_modifier: symbol!(
                    b"gbm_bo_get_modifier\0",
                    unsafe extern "C" fn(*mut c_void) -> u64
                ),
                bo_get_plane_count: symbol!(
                    b"gbm_bo_get_plane_count\0",
                    unsafe extern "C" fn(*mut c_void) -> c_int
                ),
                bo_map: symbol!(
                    b"gbm_bo_map\0",
                    unsafe extern "C" fn(
                        *mut c_void,
                        u32,
                        u32,
                        u32,
                        u32,
                        u32,
                        *mut u32,
                        *mut *mut c_void,
                    ) -> *mut c_void
                ),
                bo_unmap: symbol!(
                    b"gbm_bo_unmap\0",
                    unsafe extern "C" fn(*mut c_void, *mut c_void)
                ),
                library,
            }))
        }
    }
}

/// An allocator over one DRM node.
///
/// The node has to be open for reading **and** writing: `gbm_create_device` reaches the driver
/// through it and a read-only descriptor makes it fault rather than refuse.
#[derive(Debug)]
pub struct Device {
    /// The allocator, held by every buffer made from it as well as by this.
    allocator: Arc<Allocator>,
    /// The same pointer, for the calls this makes.
    ///
    /// A raw pointer here is also what keeps a `Device` from being shared between threads, which
    /// gbm's own calls are not written for.
    raw: *mut c_void,
}

/// The allocator itself, alive for as long as anything made from it is.
///
/// **gbm dispatches `gbm_bo_destroy` through a function pointer inside the device.** A buffer that
/// outlives its allocator therefore destroys itself through freed memory, and the call goes to
/// whatever that memory now spells — on this target, an address like `0x2`, which is a segmentation
/// fault inside a destructor with nothing in the backtrace to say why. Nothing in Rust connects a
/// buffer to the allocator that made it: gbm hands back a pointer and the relationship is entirely
/// inside the library. So the relationship is written down here instead, and every [`Allocation`]
/// holds one of these.
#[derive(Debug)]
struct Allocator {
    /// Kept so the call below stays mapped.
    library: Arc<Library>,
    /// The allocator gbm answered.
    raw: *mut c_void,
}

impl Drop for Allocator {
    fn drop(&mut self) {
        // SAFETY: `raw` is an allocator this made, and this runs after the last buffer from it has
        // been destroyed, because each of them holds a reference to this.
        unsafe { (self.library.device_destroy)(self.raw) };
    }
}

// SAFETY: what this holds is a pointer and the mapping the calls through it live in. It is reached
// for `gbm_bo_create` only through a `&Device`, which is not `Sync`, and otherwise only to destroy
// the allocator once nothing is left that was made from it.
unsafe impl Send for Allocator {}
// SAFETY: as above.
unsafe impl Sync for Allocator {}

impl Device {
    /// Opens an allocator over `node`, which stays open for as long as this does.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not give an allocator, which is what a node with
    /// no rendering support answers.
    pub fn new(library: &Arc<Library>, node: BorrowedFd<'_>) -> Result<Self, String> {
        // SAFETY: the descriptor is a DRM node the caller holds open for at least this call, and
        // gbm keeps no reference to it past `gbm_create_device`.
        let raw = unsafe { (library.create_device)(node.as_raw_fd()) };
        if raw.is_null() {
            return Err("gbm_create_device answered nothing".to_owned());
        }
        Ok(Self {
            allocator: Arc::new(Allocator {
                library: Arc::clone(library),
                raw,
            }),
            raw,
        })
    }

    /// The allocator itself, for an interface that takes one.
    ///
    /// EGL is the caller this exists for: a display over `EGL_PLATFORM_GBM_KHR` is made from this
    /// pointer, and there is no other way to name the device it stands for. The pointer is valid for
    /// as long as this [`Device`] is, and using it after that is the caller's to avoid.
    pub fn as_ptr(&self) -> *mut c_void {
        self.raw
    }

    /// The driver behind this allocator, for a log line that says which card a buffer came from.
    pub fn backend(&self) -> String {
        // SAFETY: `raw` is an allocator this made and has not destroyed.
        let name = unsafe { (self.allocator.library.backend_name)(self.raw) };
        if name.is_null() {
            return "unnamed".to_owned();
        }
        // SAFETY: gbm answers a static, nul-terminated name.
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    }

    /// Allocates one buffer a display can scan out of and a graphics device can draw into.
    ///
    /// `format` is a fourcc, the same code a framebuffer is registered under. `layout` is which
    /// form the chain takes — see [`Layout`], which is also where the reason a caller drawing
    /// across a device boundary has to state one is written.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver refused, which it does for a size, a format or a set of
    /// uses it cannot satisfy at once. Nothing here can say which of them it was: gbm answers a
    /// null pointer and no reason.
    pub fn create(
        &self,
        width: u32,
        height: u32,
        format: u32,
        layout: Layout,
    ) -> Result<Allocation, String> {
        let bo = self.allocate(width, height, format, layout);
        if bo.is_null() {
            return Err(format!(
                "gbm refused a {width}x{height} buffer that is both drawn into and scanned out, \
                 in {layout:?} layout"
            ));
        }
        // SAFETY: every one of these reads a buffer this call just made and has not destroyed, and
        // each answers a plain scalar. One block rather than four, because they share the reason.
        //
        // `gbm_bo_get_handle` is deliberately absent. It answers a *union* by value, and the i386
        // ABI returns an aggregate through a hidden pointer rather than in registers — so a
        // declaration saying it answers a `u64` hands the buffer pointer over as that hidden
        // pointer, and the driver writes the handle through it. The handle comes from importing
        // the descriptor instead, which is what the Vulkan path does and is one call either way.
        let (planes, stride, offset, modifier) = unsafe {
            (
                (self.allocator.library.bo_get_plane_count)(bo).max(0) as usize,
                (self.allocator.library.bo_get_stride)(bo),
                (self.allocator.library.bo_get_offset)(bo, 0),
                (self.allocator.library.bo_get_modifier)(bo),
            )
        };
        let allocation = Allocation {
            library: Arc::clone(&self.allocator.library),
            // What keeps the allocator alive until this buffer has been destroyed. See
            // [`Allocator`], which is where the reason is written.
            allocator: Arc::clone(&self.allocator),
            bo,
            planes,
            stride,
            offset,
            modifier,
            descriptor: None,
        };
        if allocation.planes != 1 {
            return Err(format!(
                "gbm answered a buffer in {} memory planes, and this carries one",
                allocation.planes
            ));
        }
        Ok(allocation)
    }

    /// Asks gbm for the buffer, by the most exact call this libgbm has.
    ///
    /// **`GBM_BO_USE_LINEAR` is advice and a modifier list is a requirement.** i915 answers the
    /// flag with an X-tiled buffer whose pitch is rounded up to a power of two — 8192 bytes for a
    /// 1280-pixel row that needs 5120 — and reports `DRM_FORMAT_MOD_INVALID` for it, so nothing
    /// downstream can even see that it is tiled. Naming [`LINEAR`] in a list is the form a driver
    /// has to honour or refuse.
    ///
    /// Three calls, most exact first, because a libgbm may have any of them: the list with the
    /// uses beside it, the list alone, and the flags alone. The last is where a driver that
    /// publishes no explicit layout at all ends up, and it is the behaviour this had before.
    fn allocate(&self, width: u32, height: u32, format: u32, layout: Layout) -> *mut c_void {
        let uses = USE_SCANOUT | USE_RENDERING | layout.flag();
        if let Some(modifiers) = layout.modifiers() {
            let count = modifiers.len() as c_uint;
            if let Some(create) = self.allocator.library.bo_create_with_modifiers2 {
                // SAFETY: `raw` is an allocator this made, and `modifiers` outlives the call.
                let bo = unsafe {
                    create(
                        self.raw,
                        width,
                        height,
                        format,
                        modifiers.as_ptr(),
                        count,
                        uses,
                    )
                };
                if !bo.is_null() {
                    return bo;
                }
            }
            if let Some(create) = self.allocator.library.bo_create_with_modifiers {
                // SAFETY: as above.
                let bo =
                    unsafe { create(self.raw, width, height, format, modifiers.as_ptr(), count) };
                if !bo.is_null() {
                    return bo;
                }
            }
        }
        // SAFETY: as above, and the arguments are plain values.
        unsafe { (self.allocator.library.bo_create)(self.raw, width, height, format, uses) }
    }
}

/// One buffer, and everything the two ends have to be told about it.
#[derive(Debug)]
pub struct Allocation {
    library: Arc<Library>,
    /// The allocator this came from, which has to outlive it. See [`Allocator`].
    #[expect(
        dead_code,
        reason = "held so that gbm_bo_destroy reaches a device that still exists"
    )]
    allocator: Arc<Allocator>,
    bo: *mut c_void,
    planes: usize,
    stride: u32,
    offset: u32,
    modifier: u64,
    /// The exported descriptor, once anything has asked for one.
    descriptor: Option<OwnedFd>,
}

impl Allocation {
    /// How long a row is, in bytes.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// Where the first memory plane starts.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// The layout the driver chose, or [`IMPLICIT`] where it named none.
    pub fn modifier(&self) -> u64 {
        self.modifier
    }

    /// A descriptor for this buffer, exported once and then lent out.
    ///
    /// Exported lazily and kept, because `gbm_bo_get_fd` makes a new descriptor on every call and
    /// a frame loop asking each time would run the process out of them.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not export one, which is what a buffer that cannot
    /// leave its device answers.
    pub fn descriptor(&mut self) -> Result<BorrowedFd<'_>, String> {
        if self.descriptor.is_none() {
            // SAFETY: `bo` is a buffer this holds and has not destroyed.
            let raw = unsafe { (self.library.bo_get_fd)(self.bo) };
            if raw < 0 {
                return Err("gbm_bo_get_fd would not export this buffer".to_owned());
            }
            // SAFETY: `gbm_bo_get_fd` answers a descriptor nothing else owns.
            self.descriptor = Some(unsafe { OwnedFd::from_raw_fd(raw) });
        }
        Ok(self
            .descriptor
            .as_ref()
            .expect("just exported")
            .as_fd_borrowed())
    }
}

impl Allocation {
    /// The descriptor this was already exported as, where it was.
    ///
    /// [`Allocation::descriptor`] takes `&mut self` because the first call exports and remembers.
    /// A caller building a list out of several allocations cannot hold that borrow for each of
    /// them at once, so it exports them in one pass and reads them back here in another.
    pub fn exported(&self) -> Option<BorrowedFd<'_>> {
        self.descriptor.as_ref().map(AsFd::as_fd)
    }

    /// Reads the pixel at the top-left corner, through the allocation's own mapping.
    ///
    /// For a test that has to know whether pixels **arrived**, which is a different question from
    /// whether a draw was accepted. Asking the graphics API instead would let it answer with what
    /// it was told, and that is exactly the failure worth catching: a copy into a buffer the device
    /// cannot reach reports success, runs at an impossible speed, and leaves the memory untouched.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not map it, which a buffer in device-private
    /// memory answers.
    pub fn peek(&self) -> Result<[u8; 4], String> {
        self.peek_at(0, 0)
    }

    /// Maps the whole buffer once and reads several pixels out of it.
    ///
    /// [`Allocation::peek_at`] maps a one-pixel region per call and leaves gbm to work out where
    /// that pixel is. This maps the whole surface, takes **the stride the mapping reports**, and
    /// does the arithmetic here — so a driver that computes a per-region address differently from
    /// the pitch of its own mapping cannot make a reader disagree with itself.
    ///
    /// The two exist together because a check that reads one way and writes another cannot tell a
    /// misplaced write from a misplaced read.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not map the buffer.
    pub fn read_pixels(
        &self,
        width: u32,
        height: u32,
        points: &[(u32, u32)],
    ) -> Result<(u32, Vec<[u8; 4]>), String> {
        let mut stride = 0_u32;
        let mut token: *mut c_void = core::ptr::null_mut();
        // SAFETY: `bo` is a buffer this holds, and the region asked for is the whole of it.
        let address = unsafe {
            (self.library.bo_map)(
                self.bo,
                0,
                0,
                width,
                height,
                TRANSFER_READ,
                &raw mut stride,
                &raw mut token,
            )
        };
        if address.is_null() {
            return Err("gbm_bo_map would not map this buffer".to_owned());
        }
        let length = stride as usize * height as usize;
        // SAFETY: the mapping covers `height` rows of `stride` bytes, which is what was asked for.
        let bytes = unsafe { core::slice::from_raw_parts(address.cast::<u8>(), length) };
        let read = points
            .iter()
            .map(|(x, y)| {
                let at = *y as usize * stride as usize + *x as usize * 4;
                match bytes.get(at..at + 4) {
                    Some(pixel) => [pixel[0], pixel[1], pixel[2], pixel[3]],
                    None => [0; 4],
                }
            })
            .collect();
        // SAFETY: the token is the one the mapping answered with.
        unsafe { (self.library.bo_unmap)(self.bo, token) };
        Ok((stride, read))
    }

    /// Reads one pixel out of the buffer itself, at a place the caller names.
    ///
    /// The corner is not enough to know a frame arrived **whole**. A renderer laying its rows out
    /// at `width x 4` while the buffer's rows are further apart writes a picture that is right at
    /// the origin and sheared everywhere below it, and a check on the first pixel passes. So a
    /// check that means anything reads a pixel whose address depends on the stride.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not map the buffer.
    pub fn peek_at(&self, x: u32, y: u32) -> Result<[u8; 4], String> {
        let mut stride = 0_u32;
        let mut token: *mut c_void = core::ptr::null_mut();
        // SAFETY: `bo` is a buffer this holds. gbm maps the region that is asked for and answers
        // its own address, so the one pixel named here is what the mapping covers.
        let address = unsafe {
            (self.library.bo_map)(
                self.bo,
                x,
                y,
                1,
                1,
                TRANSFER_READ,
                &raw mut stride,
                &raw mut token,
            )
        };
        if address.is_null() {
            return Err("gbm_bo_map would not map this buffer".to_owned());
        }
        // SAFETY: the mapping covers at least the one pixel that was asked for.
        let pixel = unsafe { core::slice::from_raw_parts(address.cast::<u8>(), 4) };
        let read = [pixel[0], pixel[1], pixel[2], pixel[3]];
        // SAFETY: the token is the one the mapping answered with.
        unsafe { (self.library.bo_unmap)(self.bo, token) };
        Ok(read)
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: `bo` is a buffer this holds, dropped once. The descriptor goes with it, and the
        // kernel keeps the memory alive for as long as anything that imported it holds a reference.
        unsafe { (self.library.bo_destroy)(self.bo) };
    }
}

/// Borrowing helper, so that the lifetime on [`Allocation::descriptor`] is the allocation's.
trait AsFdBorrowed {
    fn as_fd_borrowed(&self) -> BorrowedFd<'_>;
}

impl AsFdBorrowed for OwnedFd {
    fn as_fd_borrowed(&self) -> BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(self)
    }
}

// SAFETY: gbm's own objects carry no thread affinity; what they hold is a descriptor and driver
// state reached through it. The `Arc<Library>` keeps the mapping alive across threads, and nothing
// here is shared without an exclusive borrow.
unsafe impl Send for Device {}
// SAFETY: as above.
unsafe impl Send for Allocation {}

#[cfg(test)]
mod tests {
    use super::{IMPLICIT, Library};

    /// The machine running the tests need not have it, so this asserts on the answer rather than
    /// on the outcome: what must not happen is a panic or a fault inside the loader.
    #[test]
    fn loading_answers_rather_than_faults() {
        match Library::load() {
            Ok(_) => {}
            Err(why) => assert!(!why.is_empty(), "a refusal has to say something"),
        }
    }

    #[test]
    fn the_implicit_layout_is_the_kernels_own_code() {
        assert_eq!(IMPLICIT, 0x00ff_ffff_ffff_ffff, "DRM_FORMAT_MOD_INVALID");
    }
}
