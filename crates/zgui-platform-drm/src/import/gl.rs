//! The same buffers, on a machine that has no Vulkan.
//!
//! [`Imported`](super::Imported) creates its images through Vulkan and exports them. Neither half
//! of that exists on an OpenGL adapter, so this goes the other way round: [`gbm`]
//! allocates a buffer the display already owns, and EGL imports the descriptor as the texture a
//! frame is composed into. Nothing is read back and nothing is copied by the processor, which is
//! the whole point of both paths.
//!
//! # Which card allocates
//!
//! The display's own. That is the opposite of what a single-card machine would suggest and it is
//! what the hardware says: a buffer allocated on the card that *renders* is pinned somewhere else
//! the moment the display card imports the descriptor, and the submission that would draw into it
//! is refused several calls later. On a machine where the two are one card the question does not
//! arise, and this allocates from the only device there is.
//!
//! # Which layout, and the fault an implicit one hides
//!
//! [`gbm::Layout::Linear`], stated. A layout the allocator picks for itself is named to nobody: it
//! is a property of the GEM object, so it reaches the allocator's own display engine and does not
//! reach a *second* driver importing the descriptor, which is told nothing and reads the buffer as
//! linear. On this path the allocator is the display card and the renderer may be another card
//! entirely, and that is exactly the arrangement where the two disagree — the display scans a
//! tiled buffer out correctly while the renderer draws into it as though it were linear, and the
//! screen shows a regular, patterned scramble of the frame. Nothing reports it: every call
//! succeeds.
//!
//! Linear is what both ends agree on with nothing to negotiate. It gives up whatever a tiled
//! layout is worth on hardware that scans one out faster, which is worth less than a correct
//! picture. **The refinement is to negotiate** — `eglQueryDmaBufModifiersEXT` says which layouts
//! the renderer will import, the plane's `IN_FORMATS` says which the display can scan out, and the
//! intersection goes to `gbm_bo_create_with_modifiers`. That is what
//! `modifier` does for the Vulkan path, and it is the shape this one wants.
//!
//! # Nothing here is guaranteed by a capability
//!
//! The kernel's buffer-exchange documentation is explicit that agreeing a format and a layout is
//! not enough — *"having a non-empty intersection of supported modifiers does not guarantee that
//! import will succeed into all consumers; they may have constraints beyond those implied by
//! modifiers"*. So every step below reports rather than asserts, and a caller that cannot complete
//! the set falls back to copying each frame.
//!
//! # The image is destroyed at once
//!
//! `EGL_KHR_image_base` says destroying an `EGLImage` does not disturb the siblings already made
//! from it, and the texture is such a sibling. So the image is released as soon as it is bound,
//! and nothing here has to keep an EGL display alive to clean up after itself.

use std::ffi::c_void;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

use zgui_render_wgpu::{Gpu, wgpu};

use crate::import::{FORMAT, HAL_USAGE, LABEL, USAGE, Unsupported, gbm};

/// `EGL_LINUX_DMA_BUF_EXT`, the target an imported descriptor is created against.
const LINUX_DMA_BUF: khronos_egl::Enum = 0x3270;
/// `EGL_LINUX_DRM_FOURCC_EXT`.
const DRM_FOURCC: khronos_egl::Int = 0x3271;
/// `EGL_DMA_BUF_PLANE0_FD_EXT`.
const PLANE0_FD: khronos_egl::Int = 0x3272;
/// `EGL_DMA_BUF_PLANE0_OFFSET_EXT`.
const PLANE0_OFFSET: khronos_egl::Int = 0x3273;
/// `EGL_DMA_BUF_PLANE0_PITCH_EXT`.
const PLANE0_PITCH: khronos_egl::Int = 0x3274;
/// `EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT`.
const PLANE0_MODIFIER_LO: khronos_egl::Int = 0x3443;
/// `EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT`.
const PLANE0_MODIFIER_HI: khronos_egl::Int = 0x3444;

/// `EGL_SYNC_NATIVE_FENCE_ANDROID`, the sync object that can become a descriptor.
const SYNC_NATIVE_FENCE: khronos_egl::Enum = 0x3144;
/// `EGL_SYNC_FENCE_KHR`, the one that can only be waited on here.
const SYNC_FENCE: khronos_egl::Enum = 0x30F9;
/// `EGL_NO_NATIVE_FENCE_FD_ANDROID`.
const NO_NATIVE_FENCE: khronos_egl::Int = -1;
/// `EGL_FOREVER_KHR`.
const FOREVER: u64 = u64::MAX;

/// `eglCreateImageKHR`.
type CreateImage = extern "system" fn(
    *mut c_void,
    *mut c_void,
    khronos_egl::Enum,
    *mut c_void,
    *const khronos_egl::Int,
) -> *mut c_void;
/// `eglDestroyImageKHR`.
type DestroyImage = extern "system" fn(*mut c_void, *mut c_void) -> khronos_egl::Boolean;
/// `glEGLImageTargetTexture2DOES`.
type BindImage = extern "system" fn(u32, *mut c_void);
/// `eglCreateSyncKHR`.
type CreateSync =
    extern "system" fn(*mut c_void, khronos_egl::Enum, *const khronos_egl::Int) -> *mut c_void;
/// `eglDestroySyncKHR`.
type DestroySync = extern "system" fn(*mut c_void, *mut c_void) -> khronos_egl::Boolean;
/// `eglDupNativeFenceFDANDROID`.
type DupFence = extern "system" fn(*mut c_void, *mut c_void) -> khronos_egl::Int;
/// `eglClientWaitSyncKHR`.
type ClientWait =
    extern "system" fn(*mut c_void, *mut c_void, khronos_egl::Int, u64) -> khronos_egl::Enum;

/// The fourcc a scanout buffer is allocated and registered under.
///
/// `XR24`: eight bits a channel, blue first in memory, and the fourth byte ignored. The same
/// decision [`FORMAT`] states for the texture, and the two have to agree — a buffer allocated in
/// one order and scanned out in the other reaches the screen with its red and blue exchanged, and
/// no call reports it.
pub const FOURCC: u32 = u32::from_le_bytes(*b"XR24");

/// What this graphics device can be asked to do at the end of a frame.
///
/// Four tiers, and the difference between them is **who waits**. The kernel waiting is worth more
/// than the difference in code: a frame loop that blocks until the drawing is done has given up the
/// overlap between one frame's drawing and the next frame's work, which on a machine with one
/// processor is the whole of its slack.
///
/// **A wait moved to a thread is not a wait removed.** `eglClientWaitSyncKHR` holds the driver's
/// own lock for as long as it waits, so the loop's next call into that driver blocks for the rest
/// of it — measured at 13.42 ms against a 13.46 ms wait on the machine this was written for. That
/// is why the two kernel tiers matter here and not only in principle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// A sync object exported as a descriptor, handed to the commit as the plane's `IN_FENCE_FD`.
    ///
    /// The kernel waits for the drawing and this program waits for nothing. Needs
    /// `EGL_ANDROID_native_fence_sync` on the graphics driver **and** a display that takes an
    /// in-fence, which means atomic KMS and a plane that publishes the property.
    Kernel,
    /// The same descriptor, asked of the **buffer** rather than of the graphics driver.
    ///
    /// A dma-buf carries the fences of everything writing it, and hands them over as one sync file
    /// — see [`zgui_drm::sync::writers_of`]. So this reaches the same place as [`Signal::Kernel`]
    /// on a driver that exports nothing itself, which is every OpenGL driver without
    /// `EGL_ANDROID_native_fence_sync`. It also covers a buffer more than one device writes, which
    /// is what a display repairing its own scanout buffers has.
    ///
    /// **The commit need not take a fence for this to be worth having.** Where it does, the flip
    /// carries the descriptor and the kernel waits. Where it does not, the frame loop parks on the
    /// descriptor in its own wait, beside the card and the input devices, and learns there that the
    /// frame is drawn. Neither blocks in the graphics driver, which is the whole point.
    ///
    /// Needs a kernel that serves the request, and the command stream has to be flushed before the
    /// buffer is asked.
    Written,
    /// A sync object waited on here, before the commit.
    ///
    /// `EGL_KHR_fence_sync`. The wait is this thread's, but it is a wait on a fence rather than a
    /// drain of the whole device, so anything submitted after the frame keeps running.
    Client,
    /// `glFinish`, which drains everything.
    ///
    /// The floor. Correct everywhere and the reason nothing has to check whether the tiers above
    /// are available before drawing.
    Finish,
}

/// One buffer the display scans out of and the renderer composes into.
///
/// Dropping this releases the texture, and the allocation goes with it. The kernel keeps the memory
/// for as long as the framebuffer registered over it lives, so the order the two are dropped in
/// does not matter.
#[derive(Debug)]
pub struct Drawn {
    /// What the renderer composes into.
    texture: wgpu::Texture,
    /// The buffer behind it, kept because the texture names its memory and not its lifetime.
    backing: Backing,
}

/// Where a scanout buffer came from.
///
/// Two allocators answer the same need and differ in one thing that decides correctness: the pitch
/// they choose. See [`create`] for why that matters and which is asked first.
#[derive(Debug)]
enum Backing {
    /// gbm, which states at allocation that the buffer is both drawn into and scanned out.
    Gbm(gbm::Allocation),
    /// The kernel's own allocator, whose buffers are linear by the interface's own contract.
    ///
    /// It is released through [`Drawn::release`] rather than by dropping, because destroying a GEM
    /// handle needs the device it was made on and a destructor takes no arguments.
    Dumb {
        /// The buffer, which holds the handle.
        buffer: zgui_drm::buffer::DumbBuffer,
        /// The descriptor it was exported as, kept for the framebuffer the caller registers.
        descriptor: OwnedFd,
        /// How long a row is. Read once, because the buffer answers it and this keeps the two
        /// arms answering the same way.
        stride: u32,
    },
}

impl Drawn {
    /// Returns the texture the renderer composes into.
    ///
    /// Cloning it is how a set is handed to a renderer that presents into caller-supplied
    /// textures. The buffer lives until every clone has gone.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// A descriptor for this buffer, for the display card to import and register a framebuffer.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not export one.
    pub fn descriptor(&mut self) -> Result<BorrowedFd<'_>, String> {
        match &mut self.backing {
            Backing::Gbm(allocation) => allocation.descriptor(),
            // Exported once, when the buffer was made: a dumb buffer has no lazy export to do.
            // Reborrowed as shared so the descriptor's lifetime is this buffer's rather than the
            // match binding's.
            Backing::Dumb { descriptor, .. } => {
                let held: &OwnedFd = descriptor;
                Ok(held.as_fd())
            }
        }
    }

    /// The descriptor this was already exported as, where it was.
    ///
    /// For a caller building a list out of several buffers at once — see
    /// [`gbm::Allocation::exported`].
    pub fn exported(&self) -> Option<BorrowedFd<'_>> {
        match &self.backing {
            Backing::Gbm(allocation) => allocation.exported(),
            Backing::Dumb { descriptor, .. } => Some(descriptor.as_fd()),
        }
    }

    /// How long a row is, in bytes.
    pub fn stride(&self) -> u32 {
        match &self.backing {
            Backing::Gbm(allocation) => allocation.stride(),
            Backing::Dumb { stride, .. } => *stride,
        }
    }

    /// Where the first memory plane starts.
    pub fn offset(&self) -> u32 {
        match &self.backing {
            Backing::Gbm(allocation) => allocation.offset(),
            // A dumb buffer holds one plane and it starts at the beginning.
            Backing::Dumb { .. } => 0,
        }
    }

    /// The layout the driver chose, or [`gbm::IMPLICIT`] where it named none.
    pub fn modifier(&self) -> u64 {
        match &self.backing {
            Backing::Gbm(allocation) => allocation.modifier(),
            // The kernel's allocator names no layout. It does not need to: a dumb buffer is linear
            // by the interface's own contract, which is the whole reason this arm exists.
            Backing::Dumb { .. } => gbm::IMPLICIT,
        }
    }

    /// Reads the pixel at the top-left corner out of the buffer itself.
    ///
    /// For a test that has to know whether a frame **arrived**. See [`gbm::Allocation::peek`] for
    /// why the graphics API is the wrong thing to ask.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not map the buffer.
    pub fn peek(&self) -> Result<[u8; 4], String> {
        self.peek_at(0, 0)
    }

    /// Reads the pixel at `x`, `y` out of the buffer itself.
    ///
    /// See [`gbm::Allocation::peek_at`] for why a check on the corner alone proves less than it
    /// looks like it does.
    ///
    /// # Errors
    ///
    /// Returns a message where the driver would not map the buffer.
    pub fn peek_at(&self, x: u32, y: u32) -> Result<[u8; 4], String> {
        match &self.backing {
            Backing::Gbm(allocation) => allocation.peek_at(x, y),
            Backing::Dumb { .. } => Err(
                "a dumb buffer is read through its own device rather than through gbm".to_owned(),
            ),
        }
    }

    /// Reads the pixel at `x`, `y` out of the buffer, whichever allocator made it.
    ///
    /// [`Drawn::peek_at`] reads a gbm allocation through gbm. This reads either kind, because a
    /// check that can only read one of them cannot compare them — and comparing them is the whole
    /// reason there are two.
    ///
    /// # Errors
    ///
    /// Returns a message where the buffer would not map.
    pub fn read_pixel(
        &mut self,
        device: &zgui_drm::Device,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], String> {
        match &mut self.backing {
            Backing::Gbm(allocation) => allocation.peek_at(x, y),
            Backing::Dumb { buffer, stride, .. } => {
                let at = *stride as usize * y as usize + x as usize * 4;
                let bytes = buffer.bytes(device).map_err(|error| error.to_string())?;
                bytes
                    .get(at..at + 4)
                    .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                    .ok_or_else(|| format!("({x}, {y}) is past the end of this buffer"))
            }
        }
    }

    /// Gives back what a destructor cannot.
    ///
    /// A gbm allocation releases itself when it is dropped. A dumb buffer's handle belongs to the
    /// device it was made on, and destroying it needs that device — so a caller holding one of
    /// these hands it back here. A caller that drops one instead leaks a GEM handle until the
    /// descriptor closes, which is the same shape every other DRM handle in this crate has.
    pub fn release(self, device: &zgui_drm::Device) {
        let Backing::Dumb { buffer, .. } = self.backing else {
            return;
        };
        if let Err(error) = device.destroy_dumb_buffer(buffer) {
            tracing::warn!(
                target: "zgui::platform",
                "a scanout buffer's handle could not be given back: {error}"
            );
        }
    }
}

/// Creates `count` buffers whose layout the renderer and the display both agree about.
///
/// # Why the pitch decides which allocator is used
///
/// A buffer crossing a device boundary is only correct where the two ends lay its rows out the same
/// way, and **nothing in the interfaces reports whether they do**. On the machine this was written
/// against, gbm answers `DRM_FORMAT_MOD_INVALID` for every buffer it makes, rounds a 1280-pixel row
/// up to a 8192-byte pitch, refuses `gbm_bo_create_with_modifiers` outright, and ignores
/// `GBM_BO_USE_LINEAR`. The importer then lays its rows out at `width x 4` and every call along the
/// way succeeds. What reaches the screen is a scrambled frame and no error anywhere.
///
/// One condition takes the question away rather than answering it:
///
/// > **linear, and a pitch of exactly `width x 4`.**
///
/// Under it, an importer that honours the pitch it is handed and one that computes `width x 4` for
/// itself produce the same addresses, so there is nothing left for the two ends to disagree about.
/// It is an equality on two integers rather than a measurement, and a caller either has it or does
/// not.
///
/// So the allocators are asked in turn and the first whose pitch is exact is taken:
///
/// 1. **gbm**, which states at allocation that the buffer is both drawn into and scanned out. This
///    is the ask a driver can satisfy best, and on hardware whose pitch needs no rounding it is
///    also the one that gives a tiled layout where tiling is worth having.
/// 2. **the kernel's own allocator**, whose buffers are linear by the interface's own contract.
///    What it does not state is renderability, so an import or a first submission may be refused —
///    which is an error a caller falls back from rather than a wrong picture.
///
/// A caller that gets [`Unsupported`] from both falls back to copying each frame, which is always
/// correct.
///
/// **The refinement above all of this is to negotiate**: `eglQueryDmaBufModifiersEXT` says which
/// layouts the importer accepts, the plane's `IN_FORMATS` says which the display scans out, and the
/// intersection goes to `gbm_bo_create_with_modifiers`. That is explicit rather than merely
/// unambiguous, and it is what hardware new enough to publish modifiers should use. This machine
/// publishes none on either side, so there is nothing there to intersect.
pub fn create_agreed(
    gpu: &Gpu,
    allocator: &gbm::Device,
    device: &zgui_drm::Device,
    width: u32,
    height: u32,
    count: usize,
) -> Result<Vec<Drawn>, Unsupported> {
    let exact = width * 4;
    match create(gpu, allocator, width, height, count) {
        Ok(buffers) if buffers.first().is_some_and(|first| first.stride() == exact) => Ok(buffers),
        Ok(buffers) => {
            let padded = buffers.first().map_or(0, Drawn::stride);
            drop(buffers);
            tracing::info!(
                target: "zgui::platform",
                "gbm laid a row of {width} out in {padded} bytes where it needs {exact}, so the \
                 renderer and the display would not agree about where a row begins; the kernel's \
                 own allocator is asked instead"
            );
            create_dumb(gpu, device, width, height, count)
        }
        Err(why) => {
            tracing::info!(
                target: "zgui::platform",
                "gbm would not allocate a scanout buffer, so the kernel's own allocator is asked \
                 instead: {why}"
            );
            create_dumb(gpu, device, width, height, count)
        }
    }
}

/// Creates `count` buffers through the kernel's own allocator, each imported into `gpu`.
///
/// A dumb buffer is linear by the interface's own contract and its pitch is the width rounded to a
/// small alignment, so it is the arrangement most likely to satisfy the condition
/// [`create_agreed`] states. It is still checked rather than assumed: a driver is free to pad one,
/// and some do.
///
/// # Errors
///
/// Returns [`Unsupported`], which names the step that refused.
pub fn create_dumb(
    gpu: &Gpu,
    device: &zgui_drm::Device,
    width: u32,
    height: u32,
    count: usize,
) -> Result<Vec<Drawn>, Unsupported> {
    let backend = gpu.adapter().get_info().backend;
    if backend != wgpu::Backend::Gl {
        return Err(Unsupported::Backend(backend));
    }
    let mut drawn = Vec::with_capacity(count);
    for _ in 0..count {
        let buffer = device
            .create_dumb_buffer(width, height, zgui_drm::format::Format(FOURCC))
            .map_err(|error| Unsupported::Driver {
                step: "allocating a scanout buffer through the kernel's own allocator",
                reason: error.to_string(),
            })?;
        let stride = buffer.stride();
        if stride != width * 4 {
            return Err(Unsupported::Driver {
                step: "allocating a scanout buffer the renderer and the display agree about",
                reason: format!(
                    "the kernel laid a row of {width} out in {stride} bytes where it needs {}, so \
                     neither allocator on this machine answers a layout both ends read the same way",
                    width * 4
                ),
            });
        }
        let descriptor = device
            .export_buffer(&buffer)
            .map_err(|error| Unsupported::Driver {
                step: "exporting a scanout buffer as a descriptor",
                reason: error.to_string(),
            })?;
        let texture = import(
            gpu,
            descriptor.as_fd(),
            width,
            height,
            stride,
            0,
            gbm::IMPLICIT,
        )?;
        drawn.push(Drawn {
            texture,
            backing: Backing::Dumb {
                buffer,
                descriptor,
                stride,
            },
        });
    }
    Ok(drawn)
}

/// Creates `count` buffers of `width` by `height` on `allocator`, each imported into `gpu`.
///
/// `allocator` has to be over the node the **display** is on, for the reason the module
/// documentation gives.
///
/// # Errors
///
/// Returns [`Unsupported`], which names which step refused. Whatever was built before a refusal is
/// released, so a machine that cannot do this is left as it was found.
pub fn create(
    gpu: &Gpu,
    allocator: &gbm::Device,
    width: u32,
    height: u32,
    count: usize,
) -> Result<Vec<Drawn>, Unsupported> {
    let backend = gpu.adapter().get_info().backend;
    if backend != wgpu::Backend::Gl {
        return Err(Unsupported::Backend(backend));
    }
    let mut drawn = Vec::with_capacity(count);
    for slot in 0..count {
        let mut allocation = allocator
            .create(width, height, FOURCC, gbm::Layout::Linear)
            .map_err(|reason| Unsupported::Driver {
                step: "allocating a buffer that is both drawn into and scanned out",
                reason,
            })?;
        if slot == 0 {
            // The stride is worth as much as the layout code: a pitch wider than the row needs is
            // a tiled buffer whatever the code says, and on this path that is the fault that shows
            // up as a picture rather than as an error.
            tracing::info!(
                target: "zgui::platform",
                "{} laid the display's buffers out {:#x}, {} bytes to a row of {width}",
                allocator.backend(),
                allocation.modifier(),
                allocation.stride()
            );
        }
        // Read before the descriptor is borrowed: the export borrows the allocation until the
        // descriptor is done with, and these three are what the import has to be told about it.
        let (stride, offset, modifier) = (
            allocation.stride(),
            allocation.offset(),
            allocation.modifier(),
        );
        let descriptor = allocation
            .descriptor()
            .map_err(|reason| Unsupported::Driver {
                step: "exporting the buffer as a descriptor",
                reason,
            })?;
        let texture = import(gpu, descriptor, width, height, stride, offset, modifier)?;
        drawn.push(Drawn {
            texture,
            backing: Backing::Gbm(allocation),
        });
    }
    Ok(drawn)
}

/// Which of the four ways this device can be asked to say a frame has finished.
///
/// Asked once, when the buffers are made. The answer cannot change for the life of the device, and
/// asking per frame would run the extension string through a string search every frame.
///
/// `in_fence` is whether the display takes an in-fence descriptor at all. A driver that exports one
/// and a display that cannot be handed one still leave this at [`Signal::Client`]: the descriptor
/// would have nowhere to go. `from_buffers` is whether the kernel served
/// [`zgui_drm::sync::writers_of`] on one of the buffers, asked once for the same reason.
pub fn signal(gpu: &Gpu, in_fence: bool, from_buffers: bool) -> Signal {
    let Some(extensions) = display_extensions(gpu) else {
        return Signal::Finish;
    };
    let has = |name: &str| extensions.split(' ').any(|offered| offered == name);
    if in_fence && has("EGL_KHR_fence_sync") && has("EGL_ANDROID_native_fence_sync") {
        return Signal::Kernel;
    }
    // Before the client tier and after the driver's own, and **without asking about the in-fence**:
    // a descriptor the commit cannot take is one the frame loop parks on instead, and either way
    // nothing in this process waits for the card.
    if from_buffers {
        return Signal::Written;
    }
    if has("EGL_KHR_fence_sync") {
        return Signal::Client;
    }
    Signal::Finish
}

/// Makes sure everything drawn so far has landed, and answers a descriptor the kernel can wait on.
///
/// `buffer` is the descriptor of the buffer the frame was drawn into, which only [`Signal::Written`]
/// reads. `Some` under the two kernel tiers; the other two have waited by the time this returns, and
/// a caller commits without an in-fence.
pub fn finish(gpu: &Gpu, how: Signal, buffer: Option<BorrowedFd<'_>>) -> Option<OwnedFd> {
    match how {
        Signal::Kernel => native_fence(gpu),
        // The flush is what puts the frame's fence on the buffer. Asking an unflushed buffer
        // answers a descriptor for the frame before this one, and the display would then read a
        // half-drawn picture — so the two belong together and neither is the caller's to order.
        Signal::Written => {
            flush(gpu);
            written_fence(buffer?)
        }
        Signal::Client => {
            client_wait(gpu);
            None
        }
        Signal::Finish => {
            drain(gpu);
            None
        }
    }
}

/// The descriptor for everything still writing `buffer`, where the kernel answers one.
///
/// A refusal is reported and the frame goes up without a fence, which is what every machine did
/// before this tier existed. Nothing here can wait instead: the tier was chosen because the driver
/// offers no sync object to wait on.
fn written_fence(buffer: BorrowedFd<'_>) -> Option<OwnedFd> {
    match zgui_drm::sync::writers_of(buffer) {
        Ok(fence) => fence,
        Err(refusal) => {
            tracing::warn!(
                "this frame's buffer would not say what is still writing it, so the display is \
                 told to show it at once: {refusal}"
            );
            None
        }
    }
}

/// The extensions the graphics device's EGL display offers, where it has one.
fn display_extensions(gpu: &Gpu) -> Option<String> {
    // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed. The guard is read
    // through and dropped, which its own documentation permits at any time.
    let adapter = unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }?;
    let context = adapter.adapter_context();
    let egl = context.egl_instance()?;
    let display = *context.raw_display()?;
    egl.query_string(Some(display), khronos_egl::EXTENSIONS)
        .ok()
        .map(|held| held.to_string_lossy().into_owned())
}

/// Imports one descriptor as the texture a frame is composed into.
///
/// Public because who allocated the buffer is a separate question from how it is imported: a
/// caller holding a descriptor from somewhere other than [`gbm`] — the kernel's own dumb buffer
/// allocator, say — imports it through exactly this call.
///
/// `stride`, `offset` and `modifier` describe the buffer as its allocator reported it. A `modifier`
/// of [`gbm::IMPLICIT`] names no layout, which is what an allocator that publishes none answers.
///
/// # Errors
///
/// Returns [`Unsupported`], which names the step that refused.
pub fn import_descriptor(
    gpu: &Gpu,
    descriptor: BorrowedFd<'_>,
    width: u32,
    height: u32,
    stride: u32,
    offset: u32,
    modifier: u64,
) -> Result<wgpu::Texture, Unsupported> {
    import(gpu, descriptor, width, height, stride, offset, modifier)
}

/// Imports one descriptor as the texture a frame is composed into.
fn import(
    gpu: &Gpu,
    descriptor: BorrowedFd<'_>,
    width: u32,
    height: u32,
    stride: u32,
    offset: u32,
    modifier: u64,
) -> Result<wgpu::Texture, Unsupported> {
    let driver = |step: &'static str, reason: &str| Unsupported::Driver {
        step,
        reason: reason.to_owned(),
    };

    // SAFETY: as `display_extensions`.
    let adapter = unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }
        .ok_or_else(|| driver("reaching the GL adapter", "wgpu answered no GL hal adapter"))?;
    let context = adapter.adapter_context();
    let egl = context
        .egl_instance()
        .ok_or_else(|| driver("reaching EGL", "this GL context was made outside wgpu"))?;
    let display = *context
        .raw_display()
        .ok_or_else(|| driver("reaching EGL", "this GL context was made outside wgpu"))?;

    // Loaded by hand rather than through `khronos-egl`'s own binding: the display here is EGL 1.4,
    // where this is the `KHR` call and its attribute list is `EGLint` rather than the `EGLAttrib`
    // of 1.5. Asking the 1.5 binding for it on a 1.4 display reads the list at the wrong width.
    let create: CreateImage = match egl.get_proc_address("eglCreateImageKHR") {
        // SAFETY: the name is EGL's own and the signature is the one in `eglext.h`.
        Some(address) => unsafe {
            core::mem::transmute::<extern "system" fn(), CreateImage>(address)
        },
        None => return Err(driver("importing the descriptor", "no eglCreateImageKHR")),
    };
    let destroy: DestroyImage = match egl.get_proc_address("eglDestroyImageKHR") {
        // SAFETY: as above.
        Some(address) => unsafe {
            core::mem::transmute::<extern "system" fn(), DestroyImage>(address)
        },
        None => return Err(driver("importing the descriptor", "no eglDestroyImageKHR")),
    };
    let bind: BindImage = match egl.get_proc_address("glEGLImageTargetTexture2DOES") {
        // SAFETY: as above, and the signature is the one in `GL_OES_EGL_image`.
        Some(address) => unsafe {
            core::mem::transmute::<extern "system" fn(), BindImage>(address)
        },
        None => {
            return Err(driver(
                "binding the image to a texture",
                "no glEGLImageTargetTexture2DOES, so this driver cannot make a texture of an \
                     imported buffer",
            ));
        }
    };

    #[rustfmt::skip]
    let mut attributes: Vec<khronos_egl::Int> = vec![
        DRM_FOURCC, FOURCC as khronos_egl::Int,
        khronos_egl::WIDTH, width as khronos_egl::Int,
        khronos_egl::HEIGHT, height as khronos_egl::Int,
        PLANE0_FD, descriptor.as_raw_fd(),
        PLANE0_OFFSET, offset as khronos_egl::Int,
        PLANE0_PITCH, stride as khronos_egl::Int,
    ];
    // Named only where the driver named one. A chain is either wholly implicit or wholly explicit,
    // and sending `DRM_FORMAT_MOD_INVALID` as though it were a layout is neither.
    if modifier != gbm::IMPLICIT {
        attributes.extend([
            PLANE0_MODIFIER_LO,
            (modifier & 0xffff_ffff) as khronos_egl::Int,
            PLANE0_MODIFIER_HI,
            (modifier >> 32) as khronos_egl::Int,
        ]);
    }
    attributes.push(khronos_egl::NONE);
    let image = create(
        display.as_ptr(),
        khronos_egl::NO_CONTEXT,
        LINUX_DMA_BUF,
        core::ptr::null_mut(),
        attributes.as_ptr(),
    );
    if image.is_null() {
        return Err(driver(
            "importing the descriptor",
            &format!("eglCreateImageKHR refused a {width}x{height} buffer in layout {modifier:#x}"),
        ));
    }
    let name = {
        let gl = context.lock();
        // SAFETY: the context is current for as long as `gl` lives, which is what glow asks of
        // every call through it.
        unsafe {
            use glow::HasContext as _;
            let name = gl.create_texture().map_err(|reason| Unsupported::Driver {
                step: "making a texture for the imported buffer",
                reason,
            });
            let name = match name {
                Ok(name) => name,
                Err(refusal) => {
                    destroy(display.as_ptr(), image);
                    return Err(refusal);
                }
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(name));
            bind(glow::TEXTURE_2D, image);
            let error = gl.get_error();
            // The image has siblings now, and `EGL_KHR_image_base` says destroying it leaves them
            // alone. So it goes here rather than being carried for the life of the buffer.
            destroy(display.as_ptr(), image);
            if error != glow::NO_ERROR {
                gl.delete_texture(name);
                return Err(driver(
                    "binding the image to a texture",
                    &format!("glEGLImageTargetTexture2DOES answered {error:#x}"),
                ));
            }
            name
        }
    };
    Ok(wrap(gpu, name, width, height))
}

/// Wraps a GL texture as one wgpu will compose into.
fn wrap(gpu: &Gpu, name: glow::Texture, width: u32, height: u32) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let hal = wgpu::hal::TextureDescriptor {
        label: Some(LABEL),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: HAL_USAGE,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };

    // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed; the guard is read
    // through and dropped. `texture_from_raw` asks that the name is a texture created respecting
    // the descriptor, which it is: it was made on this context immediately above, at this extent
    // and format, with one level and one layer. `None` for the callback hands ownership to wgpu,
    // which deletes the texture; the memory behind it belongs to the allocation this is stored
    // beside and outlives the texture there.
    let texture = unsafe {
        let device = gpu
            .device()
            .as_hal::<wgpu::hal::api::Gles>()
            .expect("a GL device, checked by the caller");
        device.texture_from_raw(name.0, &hal, None)
    };

    let descriptor = wgpu::TextureDescriptor {
        label: Some(LABEL),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: USAGE,
        view_formats: &[],
    };
    // SAFETY: wgpu's three requirements of a hal texture are each answered. It came from this
    // device's own hal immediately above. It was created respecting this descriptor, which states
    // the same extent, format and usage. And it is complete: the image bound storage to it before
    // this. Its *contents* are undefined, which wgpu accounts for separately. It is handed over
    // once and nothing else names it.
    unsafe {
        gpu.device()
            .create_texture_from_hal::<wgpu::hal::api::Gles>(texture, &descriptor)
    }
}

/// Signals a sync object and exports it as a descriptor the kernel can wait on.
fn native_fence(gpu: &Gpu) -> Option<OwnedFd> {
    // SAFETY: as `display_extensions`.
    let adapter = unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }?;
    let context = adapter.adapter_context();
    let egl = context.egl_instance()?;
    let display = *context.raw_display()?;

    // SAFETY: the three names are EGL's own and each signature is the one in `eglext.h`.
    let (create, duplicate, destroy) = unsafe {
        (
            core::mem::transmute::<extern "system" fn(), CreateSync>(
                egl.get_proc_address("eglCreateSyncKHR")?,
            ),
            core::mem::transmute::<extern "system" fn(), DupFence>(
                egl.get_proc_address("eglDupNativeFenceFDANDROID")?,
            ),
            core::mem::transmute::<extern "system" fn(), DestroySync>(
                egl.get_proc_address("eglDestroySyncKHR")?,
            ),
        )
    };

    let sync = {
        let gl = context.lock();
        let sync = create(
            display.as_ptr(),
            SYNC_NATIVE_FENCE,
            [khronos_egl::NONE].as_ptr(),
        );
        // The object is only placed in the command stream once it is flushed, and the descriptor
        // taken from an unflushed one names a fence nothing will ever signal.
        // SAFETY: the context is current for as long as `gl` lives.
        unsafe {
            use glow::HasContext as _;
            gl.flush();
        }
        sync
    };
    if sync.is_null() {
        return None;
    }
    let raw = duplicate(display.as_ptr(), sync);
    destroy(display.as_ptr(), sync);
    if raw == NO_NATIVE_FENCE {
        return None;
    }
    // SAFETY: `eglDupNativeFenceFDANDROID` answers a descriptor nothing else owns.
    Some(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// Waits here until everything drawn so far has landed, on a fence rather than on the device.
fn client_wait(gpu: &Gpu) {
    // SAFETY: as `display_extensions`.
    let Some(adapter) = (unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }) else {
        return;
    };
    let context = adapter.adapter_context();
    let (Some(egl), Some(display)) = (context.egl_instance(), context.raw_display()) else {
        return;
    };
    let display = *display;
    let (Some(create), Some(wait), Some(destroy)) = (
        egl.get_proc_address("eglCreateSyncKHR"),
        egl.get_proc_address("eglClientWaitSyncKHR"),
        egl.get_proc_address("eglDestroySyncKHR"),
    ) else {
        return drain(gpu);
    };
    // SAFETY: the three names are EGL's own and each signature is the one in `eglext.h`.
    let (create, wait, destroy) = unsafe {
        (
            core::mem::transmute::<extern "system" fn(), CreateSync>(create),
            core::mem::transmute::<extern "system" fn(), ClientWait>(wait),
            core::mem::transmute::<extern "system" fn(), DestroySync>(destroy),
        )
    };

    let gl = context.lock();
    let sync = create(display.as_ptr(), SYNC_FENCE, [khronos_egl::NONE].as_ptr());
    if sync.is_null() {
        // SAFETY: the context is current for as long as `gl` lives.
        unsafe {
            use glow::HasContext as _;
            gl.finish();
        }
        return;
    }
    // A flush before the wait, because a fence that is still in this thread's command buffer is one
    // nothing has begun to signal, and the wait would then be for as long as the timeout allows.
    // SAFETY: as above.
    unsafe {
        use glow::HasContext as _;
        gl.flush();
    }
    wait(display.as_ptr(), sync, 0, FOREVER);
    destroy(display.as_ptr(), sync);
}

/// Sends everything recorded so far to the kernel, and waits for none of it.
///
/// `pub(crate)` because a display that composites its own frames has to flush before it asks its
/// own device to read what this one wrote.
///
/// What [`Signal::Written`] needs: a buffer carries a fence for a frame the kernel has been given,
/// and a command stream still sitting in this process reaches no buffer at all.
pub(crate) fn flush(gpu: &Gpu) {
    // SAFETY: as `display_extensions`.
    let Some(adapter) = (unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }) else {
        return;
    };
    let gl = adapter.adapter_context().lock();
    // SAFETY: the context is current for as long as `gl` lives.
    unsafe {
        use glow::HasContext as _;
        gl.flush();
    }
}

/// Drains the device, which is correct everywhere and costs the most.
fn drain(gpu: &Gpu) {
    // SAFETY: as `display_extensions`.
    let Some(adapter) = (unsafe { gpu.adapter().as_hal::<wgpu::hal::api::Gles>() }) else {
        return;
    };
    let gl = adapter.adapter_context().lock();
    // SAFETY: the context is current for as long as `gl` lives.
    unsafe {
        use glow::HasContext as _;
        gl.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::{FOURCC, Signal};
    use zgui_render_wgpu::wgpu;

    /// A buffer allocated in one channel order and scanned out in the other reaches the screen with
    /// its red and blue exchanged, and no call anywhere reports it.
    #[test]
    fn the_fourcc_and_the_texture_format_name_the_same_channel_order() {
        assert_eq!(FOURCC, u32::from_le_bytes(*b"XR24"));
        assert!(
            matches!(
                super::FORMAT,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            ),
            "XR24 reaches memory as B, G, R, x, so the texture has to store blue first"
        );
    }

    /// The floor exists so that nothing has to check the tiers above before it draws.
    #[test]
    fn every_tier_is_ordered_by_who_waits() {
        assert_ne!(Signal::Kernel, Signal::Client);
        assert_ne!(Signal::Client, Signal::Finish);
    }
}
