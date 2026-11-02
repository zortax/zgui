//! A copier that is a graphics context of its own, on the device the buffers live on.
//!
//! The context here is **not** the one the application renders with. It is opened on the display's
//! own node, holds nothing but the scanout buffers, and exists to move rectangles between them. On
//! a machine that renders on one card and displays on another that is the whole point: the
//! renderer's context is on the far side of the link, and a copy made there would cross the link
//! twice to save crossing it once.
//!
//! # What it asks of a driver
//!
//! Very little, and deliberately. A rectangle copy needs no shader, no depth buffer and no window:
//! `EGL_KHR_surfaceless_context` for the last of those, `EGL_EXT_image_dma_buf_import` and
//! `GL_OES_EGL_image` to reach the buffers, and one of two ways to move pixels. The machine this
//! was written for offers GL 2.1 on a chipset from 2005 and has all of it — which is worth stating,
//! because the same machine's driver is refused by wgpu, and that refusal is about GL 3.3 core
//! rather than about anything here.
//!
//! # Threads
//!
//! An EGL context is current on one thread at a time, and the application already holds one on the
//! rendering device. So this stays on the thread that made it, which a caller wants anyway: a
//! repair that runs on its own thread overlaps the frame being composed on the other card.

use std::ffi::c_void;
use std::marker::PhantomData;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

use crate::buffer::{Buffer, Rect};
use crate::copier::Copier;
use crate::error::Error;

/// The EGL this crate is written against.
type Instance = khronos_egl::DynamicInstance<khronos_egl::EGL1_5>;

/// `EGL_PLATFORM_GBM_KHR`.
const PLATFORM_GBM: khronos_egl::Enum = 0x31D7;
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

/// `GL_TEXTURE_2D`.
const TEXTURE_2D: u32 = 0x0DE1;
/// `GL_TEXTURE_MIN_FILTER`.
const MIN_FILTER: u32 = 0x2801;
/// `GL_TEXTURE_MAG_FILTER`.
const MAG_FILTER: u32 = 0x2800;
/// `GL_NEAREST`, which is the only sane filter for a copy that changes no size.
const NEAREST: i32 = 0x2600;
/// `GL_READ_FRAMEBUFFER`.
const READ_FRAMEBUFFER: u32 = 0x8CA8;
/// `GL_DRAW_FRAMEBUFFER`.
const DRAW_FRAMEBUFFER: u32 = 0x8CA9;
/// `GL_COLOR_ATTACHMENT0`.
const COLOR_ATTACHMENT0: u32 = 0x8CE0;
/// `GL_COLOR_BUFFER_BIT`.
const COLOR_BUFFER_BIT: u32 = 0x4000;
/// `GL_FRAMEBUFFER_COMPLETE`.
const FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;

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
/// `glEGLImageTargetTexture2DOES`.
type ImageTargetTexture = extern "system" fn(u32, *mut c_void);
/// `glCopyImageSubData`, under any of its names.
type CopyImageSubData =
    extern "system" fn(u32, u32, i32, i32, i32, i32, u32, u32, i32, i32, i32, i32, i32, i32, i32);
/// `glBlitFramebuffer`, under any of its names.
type BlitFramebuffer = extern "system" fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32);

/// The ordinary GL entry points a copy needs.
struct Gl {
    gen_textures: extern "system" fn(i32, *mut u32),
    delete_textures: extern "system" fn(i32, *const u32),
    bind_texture: extern "system" fn(u32, u32),
    tex_parameteri: extern "system" fn(u32, u32, i32),
    gen_framebuffers: extern "system" fn(i32, *mut u32),
    delete_framebuffers: extern "system" fn(i32, *const u32),
    bind_framebuffer: extern "system" fn(u32, u32),
    framebuffer_texture_2d: extern "system" fn(u32, u32, u32, u32, i32),
    check_framebuffer_status: extern "system" fn(u32) -> u32,
    finish: extern "system" fn(),
    flush: extern "system" fn(),
    get_error: extern "system" fn() -> u32,
}

/// The ways this device offers to move a rectangle from one texture to another.
///
/// **Which one works is settled by trying it, not by asking.** `glCopyImageSubData` is the cheaper
/// of the two — one call against two binds and a state change — and a driver that advertises it
/// still answers `GL_INVALID_OPERATION` for a texture that came from an `EGLImage`, because the
/// call is written in terms of internal formats and an imported image has none it recognises. No
/// extension string says so. The first copy that fails downgrades the copier for the rest of its
/// life and is run again the cheap way, which costs one copy once.
struct Ways {
    /// `glCopyImageSubData`, from `ARB_copy_image`, `EXT_copy_image` or `NV_copy_image`.
    direct: Option<CopyImageSubData>,
    /// `glBlitFramebuffer`, with a framebuffer per buffer.
    blit: Option<BlitFramebuffer>,
    /// Whether [`Ways::direct`] is still believed.
    trust_direct: bool,
}

/// What a started copy is waited on by.
///
/// Held between the two halves of a copy. The sync object is kept beside the descriptor because
/// both have to be destroyed, and destroying it is what the wait at the other end does.
enum Pending {
    /// An `EGLSync`, waited on with `eglClientWaitSyncKHR`.
    Sync(*mut c_void),
    /// The same, and a descriptor duplicated out of it that the kernel could wait on instead.
    Fence(*mut c_void, OwnedFd),
    /// Nothing to wait for: the copy is already done, or the device would make nothing.
    Nothing,
}

/// How this device says a copy is done.
///
/// A descriptor is preferred wherever the driver offers one, GL or otherwise: it is the difference
/// between the kernel waiting for the copy and this thread waiting for it.
enum Told {
    /// `EGL_ANDROID_native_fence_sync`, which becomes a descriptor.
    Descriptor {
        /// `eglCreateSyncKHR`.
        create: CreateSync,
        /// `eglDestroySyncKHR`.
        destroy: DestroySync,
        /// `eglDupNativeFenceFDANDROID`.
        dup: DupFence,
    },
    /// `EGL_KHR_fence_sync`, waited on here — a wait on one fence rather than a drain of the device.
    Fence {
        /// `eglCreateSyncKHR`.
        create: CreateSync,
        /// `eglDestroySyncKHR`.
        destroy: DestroySync,
        /// `eglClientWaitSyncKHR`.
        wait: ClientWait,
    },
    /// `glFinish`. The floor, and correct everywhere.
    Drain,
}

/// What had the thread's EGL context before a copy took it.
///
/// Its display, its context and its two surfaces. [`None`] where nothing was current.
type Held = Option<(
    khronos_egl::Display,
    Option<khronos_egl::Context>,
    Option<khronos_egl::Surface>,
    Option<khronos_egl::Surface>,
)>;

/// Reads what currently has the thread, so that a copy can put it back.
///
/// **EGL binds a context to a thread, and the caller has one of its own.** A renderer on the same
/// thread has made its context current since this copier opened, so a copy issued without taking
/// the thread would run against *that* context: the entry points are dispatch stubs that act on
/// whatever is current, and the texture names would name whatever the other context has under those
/// numbers. It is silent when it happens — every call is accepted and the pixels never move.
fn held_by_the_thread(egl: &Instance) -> Held {
    egl.get_current_display().map(|display| {
        (
            display,
            egl.get_current_context(),
            egl.get_current_surface(khronos_egl::DRAW),
            egl.get_current_surface(khronos_egl::READ),
        )
    })
}

/// Puts back what [`held_by_the_thread`] read, or releases the thread where nothing had it.
fn put_the_thread_back(egl: &Instance, mine: khronos_egl::Display, held: Held) {
    match held {
        Some((display, context, draw, read)) => {
            let _ = egl.make_current(display, draw, read, context);
        }
        // The thread is the caller's rather than this copier's, so it is left holding nothing
        // rather than left holding this.
        None => {
            let _ = egl.make_current(mine, None, None, None);
        }
    }
}

/// A graphics context on the display's own device, holding the buffers it copies between.
pub struct Egl {
    /// EGL, loaded.
    egl: Instance,
    /// The display, made over [`Allocator::raw`].
    display: khronos_egl::Display,
    /// The context every call below is made under.
    context: khronos_egl::Context,
    /// The ordinary entry points.
    gl: Gl,
    /// `glEGLImageTargetTexture2DOES`.
    bind_image: ImageTargetTexture,
    /// `eglDestroyImageKHR`, kept for the drop.
    destroy_image: DestroyImage,
    /// How a rectangle is moved.
    how: Ways,
    /// How the caller is told it is done.
    told: Told,
    /// What the copy started by `begin` is waited on by, while one is running.
    pending: Pending,
    /// One `EGLImage` per buffer.
    images: Vec<*mut c_void>,
    /// One texture per buffer.
    textures: Vec<u32>,
    /// One framebuffer per buffer, made wherever `glBlitFramebuffer` exists to need them.
    framebuffers: Vec<u32>,
    /// The allocator the display was made over.
    ///
    /// Held rather than read: an EGL display made over a gbm device is only as valid as the device
    /// is, so this outlives the display and is dropped after it.
    #[expect(dead_code, reason = "holds the device the display was made over")]
    allocator: zgui_gbm::Device,
    /// What makes this `!Send` and `!Sync`. The raw pointers do it too; this states it.
    thread_bound: PhantomData<*const ()>,
}

impl std::fmt::Debug for Egl {
    /// What a log line about a copier needs: how many buffers it holds and how it moves them.
    ///
    /// Written out rather than derived, because EGL's own handles describe nothing and the function
    /// pointers describe less.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Egl")
            .field("buffers", &self.textures.len())
            // What it will *try*. A copier fresh from `open` has run no copy yet, and this device
            // is only believed about `glCopyImageSubData` until the first one it refuses.
            .field(
                "tries",
                &if self.how.trust_direct {
                    "glCopyImageSubData"
                } else {
                    "glBlitFramebuffer"
                },
            )
            .field(
                "signals",
                &match self.told {
                    Told::Descriptor { .. } => "a descriptor",
                    Told::Fence { .. } => "a fence waited on here",
                    Told::Drain => "glFinish",
                },
            )
            .finish()
    }
}

impl Egl {
    /// Opens a copier on `node`, holding `buffers`.
    ///
    /// `node` is the device the buffers live on, which for a scanout buffer is the **display's**
    /// node. Every buffer is imported once, here, because importing is what costs and a frame loop
    /// copies between the same buffers for the life of a mode.
    ///
    /// # Errors
    ///
    /// Returns [`Error`], naming the step that refused. Nothing is left behind on a refusal: a
    /// machine that cannot do this is left exactly as it was found, and its caller sends the pixels
    /// the long way round.
    pub fn open(node: BorrowedFd<'_>, buffers: &[Buffer<'_>]) -> Result<Self, Error> {
        // SAFETY: opening a library runs its initialisers, which is what this crate exists to do
        // rather than link. The name is a soname and not a path, so the loader's own search decides
        // what is opened.
        let library = zgui_gbm::Library::load().map_err(|reason| Error::Library {
            soname: "libgbm.so.1",
            reason,
        })?;
        let allocator = zgui_gbm::Device::new(&library, node).map_err(|reason| Error::Driver {
            step: "opening an allocator over the display's own node",
            reason,
        })?;

        // SAFETY: the load this crate exists to do rather than link.
        let egl = unsafe { Instance::load_required() }.map_err(|reason| Error::Library {
            soname: "libEGL.so.1",
            reason: reason.to_string(),
        })?;
        // SAFETY: the platform is the one `zgui_gbm`'s pointer belongs to, and the allocator
        // outlives the display because this holds it and drops it afterwards.
        let display = unsafe {
            egl.get_platform_display(
                PLATFORM_GBM,
                allocator.as_ptr(),
                &[khronos_egl::ATTRIB_NONE],
            )
        }
        .map_err(|reason| Error::Driver {
            step: "asking EGL for a display over that node",
            reason: reason.to_string(),
        })?;
        egl.initialize(display).map_err(|reason| Error::Driver {
            step: "initialising EGL on that node",
            reason: reason.to_string(),
        })?;

        let offered = egl
            .query_string(Some(display), khronos_egl::EXTENSIONS)
            .map(|text| text.to_string_lossy().into_owned())
            .unwrap_or_default();
        for needed in [
            "EGL_EXT_image_dma_buf_import",
            "EGL_KHR_surfaceless_context",
        ] {
            if !has(&offered, needed) {
                return Err(Error::Driver {
                    step: "checking what this device offers",
                    reason: format!("no {needed}"),
                });
            }
        }

        let context = context(&egl, display)?;
        egl.make_current(display, None, None, Some(context))
            .map_err(|reason| Error::Driver {
                step: "making the copier's context current",
                reason: reason.to_string(),
            })?;

        let gl = Gl::resolve(&egl)?;
        let bind_image: ImageTargetTexture = resolve(&egl, &["glEGLImageTargetTexture2DOES"])
            .ok_or(Error::Symbol {
                name: "glEGLImageTargetTexture2DOES",
            })?;
        let create_image: CreateImage = resolve(&egl, &["eglCreateImageKHR", "eglCreateImage"])
            .ok_or(Error::Symbol {
                name: "eglCreateImageKHR",
            })?;
        let destroy_image: DestroyImage = resolve(&egl, &["eglDestroyImageKHR", "eglDestroyImage"])
            .ok_or(Error::Symbol {
                name: "eglDestroyImageKHR",
            })?;
        let how = Ways::resolve(&egl)?;
        let told = Told::resolve(&egl, &offered);

        let mut copier = Self {
            egl,
            display,
            context,
            gl,
            bind_image,
            destroy_image,
            how,
            told,
            pending: Pending::Nothing,
            images: Vec::with_capacity(buffers.len()),
            textures: Vec::with_capacity(buffers.len()),
            framebuffers: Vec::new(),
            allocator,
            thread_bound: PhantomData,
        };
        copier.attach(create_image, buffers)?;
        Ok(copier)
    }

    /// Imports every buffer as a texture, and a framebuffer over it where the copy needs one.
    fn attach(&mut self, create: CreateImage, buffers: &[Buffer<'_>]) -> Result<(), Error> {
        for buffer in buffers {
            let mut attributes = vec![
                khronos_egl::WIDTH,
                buffer.width as khronos_egl::Int,
                khronos_egl::HEIGHT,
                buffer.height as khronos_egl::Int,
                DRM_FOURCC,
                buffer.fourcc as khronos_egl::Int,
                PLANE0_FD,
                buffer.descriptor.as_raw_fd(),
                PLANE0_OFFSET,
                buffer.offset as khronos_egl::Int,
                PLANE0_PITCH,
                buffer.stride as khronos_egl::Int,
            ];
            // Named only where the allocator named one: a chain is wholly implicit or wholly
            // explicit, and stating a layout on an implicit buffer is how a picture comes out
            // striped with nothing to report it.
            if let Some(modifier) = buffer.modifier {
                attributes.extend([
                    PLANE0_MODIFIER_LO,
                    (modifier & 0xffff_ffff) as khronos_egl::Int,
                    PLANE0_MODIFIER_HI,
                    (modifier >> 32) as khronos_egl::Int,
                ]);
            }
            attributes.push(khronos_egl::NONE);

            let image = create(
                self.display.as_ptr(),
                khronos_egl::NO_CONTEXT,
                LINUX_DMA_BUF,
                std::ptr::null_mut(),
                attributes.as_ptr(),
            );
            if image.is_null() {
                return Err(Error::Driver {
                    step: "importing a scanout buffer as an image",
                    reason: "eglCreateImageKHR answered nothing".to_owned(),
                });
            }
            self.images.push(image);

            let mut texture = 0;
            (self.gl.gen_textures)(1, &raw mut texture);
            (self.gl.bind_texture)(TEXTURE_2D, texture);
            (self.gl.tex_parameteri)(TEXTURE_2D, MIN_FILTER, NEAREST);
            (self.gl.tex_parameteri)(TEXTURE_2D, MAG_FILTER, NEAREST);
            (self.bind_image)(TEXTURE_2D, image);
            if (self.gl.get_error)() != 0 {
                return Err(Error::Driver {
                    step: "binding an imported image as a texture",
                    reason: "glEGLImageTargetTexture2DOES refused it".to_owned(),
                });
            }
            self.textures.push(texture);

            if self.how.blit.is_some() {
                let mut framebuffer = 0;
                (self.gl.gen_framebuffers)(1, &raw mut framebuffer);
                (self.gl.bind_framebuffer)(DRAW_FRAMEBUFFER, framebuffer);
                (self.gl.framebuffer_texture_2d)(
                    DRAW_FRAMEBUFFER,
                    COLOR_ATTACHMENT0,
                    TEXTURE_2D,
                    texture,
                    0,
                );
                let status = (self.gl.check_framebuffer_status)(DRAW_FRAMEBUFFER);
                if status != FRAMEBUFFER_COMPLETE {
                    return Err(Error::Driver {
                        step: "making a framebuffer over an imported buffer",
                        reason: format!("the driver answered 0x{status:x}"),
                    });
                }
                self.framebuffers.push(framebuffer);
            }
        }
        Ok(())
    }

    /// Makes something the copy can be waited on by, without waiting for it here.
    ///
    /// **Nothing blocks in this call.** The whole reason the copy is in two halves is that about
    /// three quarters of it is the device working while the processor has nothing to do — measured
    /// on the hardware this was written for at 1.07 ms of wall against 0.29 ms of processor. Those
    /// three quarters belong to whatever the caller does next.
    fn started(&self) -> Pending {
        match &self.told {
            Told::Descriptor { create, dup, .. } => {
                let sync = create(
                    self.display.as_ptr(),
                    SYNC_NATIVE_FENCE,
                    [NO_NATIVE_FENCE, khronos_egl::NONE].as_ptr(),
                );
                if sync.is_null() {
                    (self.gl.finish)();
                    return Pending::Nothing;
                }
                // The commands have to be on their way before the descriptor is asked for, which is
                // what the extension says and what makes the answer mean anything.
                (self.gl.flush)();
                let raw = dup(self.display.as_ptr(), sync);
                if raw < 0 {
                    (self.gl.finish)();
                    return Pending::Sync(sync);
                }
                // SAFETY: the descriptor is this process's own, answered by the driver and owned by
                // nothing else.
                Pending::Fence(sync, unsafe { OwnedFd::from_raw_fd(raw) })
            }
            Told::Fence { create, .. } => {
                let sync = create(
                    self.display.as_ptr(),
                    SYNC_FENCE,
                    [khronos_egl::NONE].as_ptr(),
                );
                if sync.is_null() {
                    (self.gl.finish)();
                    return Pending::Nothing;
                }
                // Flushed rather than finished: this is what puts the work on its way so that the
                // wait at the other end has something to wait for.
                (self.gl.flush)();
                Pending::Sync(sync)
            }
            // The floor. There is nothing to wait on later, so the wait is here and the caller's
            // next work does not overlap it.
            Told::Drain => {
                (self.gl.finish)();
                Pending::Nothing
            }
        }
    }
}

impl Egl {
    /// Runs one pass of `rects`, the way `trust_direct` currently says, and reports the device's
    /// verdict on it.
    fn pass(&self, from: usize, to: usize, rects: &[Rect]) -> u32 {
        // Drained first, so what comes back afterwards belongs to this pass and to nothing before
        // it.
        while (self.gl.get_error)() != 0 {}
        match (self.how.trust_direct, self.how.direct, self.how.blit) {
            (true, Some(copy), _) => {
                for rect in rects.iter().filter(|rect| !rect.is_empty()) {
                    copy(
                        self.textures[from],
                        TEXTURE_2D,
                        0,
                        rect.x,
                        rect.y,
                        0,
                        self.textures[to],
                        TEXTURE_2D,
                        0,
                        rect.x,
                        rect.y,
                        0,
                        rect.width,
                        rect.height,
                        1,
                    );
                }
            }
            (_, _, Some(blit)) => {
                (self.gl.bind_framebuffer)(READ_FRAMEBUFFER, self.framebuffers[from]);
                (self.gl.bind_framebuffer)(DRAW_FRAMEBUFFER, self.framebuffers[to]);
                for rect in rects.iter().filter(|rect| !rect.is_empty()) {
                    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
                    blit(
                        rect.x,
                        rect.y,
                        right,
                        bottom,
                        rect.x,
                        rect.y,
                        right,
                        bottom,
                        COLOR_BUFFER_BIT,
                        NEAREST as u32,
                    );
                }
            }
            _ => return 0,
        }
        (self.gl.get_error)()
    }
}

impl Copier for Egl {
    fn begin(&mut self, from: usize, to: usize, rects: &[Rect]) -> Result<(), Error> {
        self.begin_all(&[(from, to, rects)])
    }

    /// Every step under one take of the device and behind one fence.
    ///
    /// Taking the thread's context and giving it back is the expensive part of an issue, and a
    /// fence placed after the last step covers every step before it — the passes are recorded on
    /// one context and run in the order they were given. So a picture built out of two copies costs
    /// one issue rather than two, and the caller waits once or not at all.
    fn begin_all(&mut self, steps: &[(usize, usize, &[Rect])]) -> Result<(), Error> {
        let held = self.textures.len();
        for (from, to, _) in steps {
            if *from >= held || *to >= held {
                return Err(Error::Driver {
                    step: "copying between two of this copier's buffers",
                    reason: format!("asked for {from} and {to} of {held}"),
                });
            }
        }
        // **The copy already running is let go of rather than waited for.** Waiting here would be
        // waiting for the frame before this one — and on the arrangement this exists for, that copy
        // is itself waiting for the graphics device to finish the frame it reads. One wait becomes
        // two devices deep, on the frame loop's own thread.
        //
        // Letting go is sound because the fence placed below covers it too: the passes are recorded
        // on one context and run in the order they were given, so a fence after the last of them
        // has the earlier ones behind it. What a caller loses is the ability to wait for one
        // specific copy, which no caller of this asks for.
        self.let_go();
        if steps.is_empty() {
            return Ok(());
        }

        // Taken for the whole of the issue and put back at the one exit below — everything between
        // runs on this copier's own context, and the *caller's* is current again by the time this
        // answers. That is what lets the caller work while the device copies.
        let held = held_by_the_thread(&self.egl);
        if let Err(reason) = self
            .egl
            .make_current(self.display, None, None, Some(self.context))
        {
            // Nothing was changed, so there is nothing to put back.
            return Err(Error::Driver {
                step: "making this copier's context current for a copy",
                reason: reason.to_string(),
            });
        }
        let mut answered = 0;
        for (from, to, rects) in steps {
            answered = self.pass(*from, *to, rects);
            // The downgrade, and it happens once: see [`Ways`] for why no string could have said
            // this in advance.
            if answered != 0 && self.how.trust_direct && self.how.blit.is_some() {
                self.how.trust_direct = false;
                answered = self.pass(*from, *to, rects);
            }
            if answered != 0 {
                break;
            }
        }
        if answered == 0 {
            self.pending = self.started();
        }
        put_the_thread_back(&self.egl, self.display, held);
        if answered == 0 {
            Ok(())
        } else {
            Err(Error::Driver {
                step: "copying between two scanout buffers",
                reason: format!("the device answered 0x{answered:x}"),
            })
        }
    }

    fn finish(&mut self) -> Result<(), Error> {
        // Neither half of this needs the context: both are asked of the display, and the caller has
        // its own context current by now. That is the point — nothing here disturbs the caller.
        match std::mem::replace(&mut self.pending, Pending::Nothing) {
            Pending::Nothing => Ok(()),
            Pending::Sync(sync) => {
                self.wait_for(sync);
                Ok(())
            }
            Pending::Fence(sync, fence) => {
                // The descriptor is this copier's to wait on, because nothing else was given it. A
                // display that took an in-fence would be handed this instead, and then there would
                // be nothing to wait for at all.
                let mut polled = [rustix::event::PollFd::new(
                    &fence,
                    rustix::event::PollFlags::IN,
                )];
                let waited = rustix::event::poll(&mut polled, None);
                self.wait_for(sync);
                waited.map(|_| ()).map_err(|errno| Error::Driver {
                    step: "waiting for a copy's fence",
                    reason: errno.to_string(),
                })
            }
        }
    }

    fn len(&self) -> usize {
        self.textures.len()
    }
}

impl Egl {
    /// Gives up whatever a running copy left behind, without waiting for it.
    ///
    /// The sync object is destroyed and any descriptor is closed. Destroying a sync object does not
    /// cancel the work behind it — the copy runs to its end either way — and what is given up is
    /// only the ability to be told *when*, which the fence placed after the next issue answers for
    /// both. See [`Copier::begin_all`](crate::Copier::begin_all) for why this is not a wait.
    fn let_go(&mut self) {
        match core::mem::replace(&mut self.pending, Pending::Nothing) {
            Pending::Nothing => {}
            Pending::Sync(sync) | Pending::Fence(sync, _) => match &self.told {
                Told::Descriptor { destroy, .. } | Told::Fence { destroy, .. } => {
                    let _ = destroy(self.display.as_ptr(), sync);
                }
                Told::Drain => {}
            },
        }
    }

    /// Waits for one sync object and destroys it.
    fn wait_for(&self, sync: *mut c_void) {
        match &self.told {
            Told::Descriptor { destroy, .. } => {
                let _ = destroy(self.display.as_ptr(), sync);
            }
            Told::Fence { destroy, wait, .. } => {
                let _ = wait(self.display.as_ptr(), sync, 0, FOREVER);
                let _ = destroy(self.display.as_ptr(), sync);
            }
            Told::Drain => {}
        }
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        if !self.framebuffers.is_empty() {
            (self.gl.delete_framebuffers)(
                self.framebuffers.len() as i32,
                self.framebuffers.as_ptr(),
            );
        }
        if !self.textures.is_empty() {
            (self.gl.delete_textures)(self.textures.len() as i32, self.textures.as_ptr());
        }
        for image in &self.images {
            let _ = (self.destroy_image)(self.display.as_ptr(), *image);
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
        let _ = self.egl.terminate(self.display);
        // The allocator goes with this, after the display made over it.
    }
}

/// Returns `true` where a space-separated extension string holds `name`.
fn has(offered: &str, name: &str) -> bool {
    offered.split_whitespace().any(|each| each == name)
}

/// Resolves the first of `names` the driver answers, as `T`.
fn resolve<T: Copy>(egl: &Instance, names: &[&str]) -> Option<T> {
    debug_assert_eq!(size_of::<T>(), size_of::<extern "system" fn()>());
    names.iter().find_map(|name| {
        let address = egl.get_proc_address(name)?;
        // SAFETY: every `T` this is asked for is a function pointer of the signature the entry
        // point named by `name` has, written out above beside the name it goes with.
        Some(unsafe { std::mem::transmute_copy(&address) })
    })
}

/// Makes a context for whichever API this device offers, desktop GL first.
///
/// Either will do. A rectangle copy is inside both, and which one a driver has is the driver's
/// business — a part that offers GL 2.1 and one that offers GLES 2.0 both service this crate.
fn context(egl: &Instance, display: khronos_egl::Display) -> Result<khronos_egl::Context, Error> {
    let mut configs = Vec::with_capacity(32);
    egl.get_configs(display, &mut configs)
        .map_err(|reason| Error::Driver {
            step: "listing this device's configs",
            reason: reason.to_string(),
        })?;

    // No surface type is asked for. The context is made current against no surface at all, which is
    // what `EGL_KHR_surfaceless_context` is for, and a display over gbm offers window configs only.
    for (api, renderable, version) in [
        (khronos_egl::OPENGL_API, khronos_egl::OPENGL_BIT, None),
        (
            khronos_egl::OPENGL_ES_API,
            khronos_egl::OPENGL_ES2_BIT,
            Some(2),
        ),
    ] {
        if egl.bind_api(api).is_err() {
            continue;
        }
        let found = configs.iter().copied().find(|config| {
            egl.get_config_attrib(display, *config, khronos_egl::RENDERABLE_TYPE)
                .is_ok_and(|bits| bits & renderable != 0)
        });
        let Some(config) = found else { continue };
        // A GLES context defaults to version 1 where nothing says otherwise, and version 1 has no
        // framebuffer objects at all.
        let attributes = match version {
            Some(version) => vec![
                khronos_egl::CONTEXT_CLIENT_VERSION,
                version,
                khronos_egl::NONE,
            ],
            None => vec![khronos_egl::NONE],
        };
        if let Ok(context) = egl.create_context(display, config, None, &attributes) {
            return Ok(context);
        }
    }
    Err(Error::Driver {
        step: "making a context on the display's device",
        reason: "no config this device offers is renderable by OpenGL or OpenGL ES 2".to_owned(),
    })
}

impl Gl {
    /// Resolves the ordinary entry points, which every one of these APIs has.
    fn resolve(egl: &Instance) -> Result<Self, Error> {
        // The suffixed names are asked for as well, because a framebuffer object is an extension on
        // the GL 2.1 this was written against and core on everything newer.
        Ok(Self {
            gen_textures: resolve(egl, &["glGenTextures"]).ok_or(Error::Symbol {
                name: "glGenTextures",
            })?,
            delete_textures: resolve(egl, &["glDeleteTextures"]).ok_or(Error::Symbol {
                name: "glDeleteTextures",
            })?,
            bind_texture: resolve(egl, &["glBindTexture"]).ok_or(Error::Symbol {
                name: "glBindTexture",
            })?,
            tex_parameteri: resolve(egl, &["glTexParameteri"]).ok_or(Error::Symbol {
                name: "glTexParameteri",
            })?,
            gen_framebuffers: resolve(egl, &["glGenFramebuffers", "glGenFramebuffersEXT"]).ok_or(
                Error::Symbol {
                    name: "glGenFramebuffers",
                },
            )?,
            delete_framebuffers: resolve(egl, &["glDeleteFramebuffers", "glDeleteFramebuffersEXT"])
                .ok_or(Error::Symbol {
                    name: "glDeleteFramebuffers",
                })?,
            bind_framebuffer: resolve(egl, &["glBindFramebuffer", "glBindFramebufferEXT"]).ok_or(
                Error::Symbol {
                    name: "glBindFramebuffer",
                },
            )?,
            framebuffer_texture_2d: resolve(
                egl,
                &["glFramebufferTexture2D", "glFramebufferTexture2DEXT"],
            )
            .ok_or(Error::Symbol {
                name: "glFramebufferTexture2D",
            })?,
            check_framebuffer_status: resolve(
                egl,
                &["glCheckFramebufferStatus", "glCheckFramebufferStatusEXT"],
            )
            .ok_or(Error::Symbol {
                name: "glCheckFramebufferStatus",
            })?,
            finish: resolve(egl, &["glFinish"]).ok_or(Error::Symbol { name: "glFinish" })?,
            flush: resolve(egl, &["glFlush"]).ok_or(Error::Symbol { name: "glFlush" })?,
            get_error: resolve(egl, &["glGetError"]).ok_or(Error::Symbol { name: "glGetError" })?,
        })
    }
}

impl Ways {
    /// Resolves both ways, and refuses a device that offers neither.
    fn resolve(egl: &Instance) -> Result<Self, Error> {
        let direct = resolve::<CopyImageSubData>(
            egl,
            &[
                "glCopyImageSubData",
                "glCopyImageSubDataEXT",
                "glCopyImageSubDataNV",
                "glCopyImageSubDataOES",
            ],
        );
        let blit = resolve::<BlitFramebuffer>(
            egl,
            &[
                "glBlitFramebuffer",
                "glBlitFramebufferEXT",
                "glBlitFramebufferNV",
                "glBlitFramebufferANGLE",
            ],
        );
        if direct.is_none() && blit.is_none() {
            return Err(Error::NoCopy);
        }
        Ok(Self {
            direct,
            blit,
            trust_direct: direct.is_some(),
        })
    }
}

impl Told {
    /// Picks the best answer this device can give, which is the one the kernel can wait on.
    fn resolve(egl: &Instance, offered: &str) -> Self {
        let create = resolve::<CreateSync>(egl, &["eglCreateSyncKHR", "eglCreateSync"]);
        let destroy = resolve::<DestroySync>(egl, &["eglDestroySyncKHR", "eglDestroySync"]);
        let (Some(create), Some(destroy)) = (create, destroy) else {
            return Self::Drain;
        };
        if has(offered, "EGL_ANDROID_native_fence_sync")
            && let Some(dup) = resolve::<DupFence>(egl, &["eglDupNativeFenceFDANDROID"])
        {
            return Self::Descriptor {
                create,
                destroy,
                dup,
            };
        }
        if has(offered, "EGL_KHR_fence_sync")
            && let Some(wait) =
                resolve::<ClientWait>(egl, &["eglClientWaitSyncKHR", "eglClientWaitSync"])
        {
            return Self::Fence {
                create,
                destroy,
                wait,
            };
        }
        Self::Drain
    }
}
