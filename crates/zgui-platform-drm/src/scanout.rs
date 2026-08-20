//! Where a frame lands, and the flip that puts it on the screen.
//!
//! A display scans out of one buffer while the next frame is written into another, and a flip
//! swaps which is which at the vertical blank. [`Scanout`] owns that rotation for one output: the
//! buffers, the framebuffers the kernel knows them by, what each buffer is doing, and whether the
//! mode has been set at all.
//!
//! # Which buffer a frame goes into
//!
//! `rotation` holds that decision, and it holds it apart from the card so that it can be asserted
//! with no hardware. What is left here is the driving: the barriers, the copy, the commit and the
//! fence.
//!
//! One thing follows from it that reaches every caller. A frame that finishes while a flip is still
//! on its way is **held** — the kernel takes one page flip per CRTC — and it goes to the driver
//! when the completion arrives, in its own buffer and with its own fence. [`Scanout::drain`] is
//! where that happens, and it commits for that reason.
//!
//! # Two shapes
//!
//! **The copied shape.** Two buffers the driver allocated and the processor writes. A frame is
//! read back out of the renderer and copied in, the pointer is drawn over it, and the flip
//! follows. It costs a readback and eight megabytes of copying a frame, and it is the answer for
//! every machine where the other shape cannot be built.
//!
//! **The imported shape.** Three Vulkan images the renderer composes straight into, exported as
//! dma-buf descriptors and registered as framebuffers in the layout the driver chose. Nothing is
//! read back and nothing is copied. A frame there is bracketed by the two barriers
//! [`Handover`] records: one takes the buffer back from the display engine before
//! the frame is drawn, and one gives it over afterwards, and then the flip follows. The flip
//! carries a sync file for that second barrier where the display can be given one, so the kernel
//! waits for the graphics device and the frame loop's own thread waits for nothing.
//!
//! An enum holds the two, for three reasons. There are exactly two and both live here, so nothing
//! outside adds a third. [`Scanout::release`] takes itself by value, which a trait object states
//! only through `Box<Self>`. And the two are presented to differently — one takes the pixels of a
//! frame and the other takes none, because the frame is already in the buffer — so there is no one
//! method for a trait to hold.
//!
//! # The mode is set by the first present
//!
//! On both shapes. The imported shape cannot do it any earlier: its images belong to the graphics
//! device, which exists only once the renderer has been built, which happens after the loop that
//! would have set the mode has started. Deferring it removes the ordering constraint from both
//! shapes at once, and it leaves the console's own text on the screen until there is a frame to
//! replace it with.
//!
//! A modeset carries no completion event — the call returning says it finished — so the first
//! present leaves nothing outstanding and the second present is taken at once.
//!
//! # Coming back after another session had the screen
//!
//! [`Scanout::restore`] is the other place a mode is set. A person who switches terminal gives the
//! CRTC to another session, which sets its own mode on it, so this one has to set its own again on
//! the way back. The buffers are this process's own and still hold the frame that was on the screen,
//! so the picture returns at that commit rather than at the frame after it.
//!
//! # The pointer decides the shape
//!
//! A display whose engine composites no pointer has it drawn into the frame, and nothing can draw
//! a pointer into a tiled image from the processor. So such a display keeps the copied shape
//! whatever else it could do. [`Copied`] is the reason a display is on it, stated so that the
//! caller can log which of them it was.
//!
//! [`FORMAT`] is the other half of the agreement with whatever draws, and it is stated here
//! because this is where it is read: a renderer drawing for this backend composes into it, and the
//! fourcc the buffers are registered under comes from it.

pub(crate) mod rotation;

use std::fmt;
use std::os::fd::{AsFd, OwnedFd};

use tracing::{info, warn};
use zgui_drm::buffer::{DumbBuffer, ImportedBuffer};
use zgui_drm::commit::{Commit, Pipe, waits_for_a_fence};
use zgui_drm::format::{Format, Modifier};
use zgui_drm::framebuffer::Framebuffer;
use zgui_drm::resources::Mode;
use zgui_drm::{Device, Event};
use zgui_platform::PlatformError;
use zgui_render_wgpu::{Gpu, Pixels, wgpu};

use crate::cursor::Cursor;
use crate::import::{Handover, Imported, Plane, Unsupported};
use crate::output::{Output, backend};
use crate::scanout::rotation::{Ready, Rotation};

/// How many buffers the copied shape drives a display from.
///
/// Two: one on the screen while the other is written. One would tear.
///
/// With two, a flip on its way leaves no buffer free, so a frame that arrives inside that window is
/// declined before the copy is made. That falls out of the same rule that holds a frame on three
/// buffers: there is one fewer buffer for the rule to reach. A third buffer would give this shape
/// the latency the imported one has, for eight megabytes and a whole-frame copy the display may
/// never read.
const COPIED: usize = 2;

/// How many buffers the imported shape drives a display from.
///
/// Three, and the third one is needed rather than spare. Here the renderer draws **into** the
/// scanout buffer, so the buffer it is given has to be neither the one on the screen nor the one an
/// outstanding flip names. With two buffers those are both of them, and a display would draw
/// nothing for as long as a flip was on its way.
///
/// So a frame here starts as soon as it is asked for, and waits for the vertical blank only to be
/// committed. `rotation` is where that is worked out.
const IMPORTED: usize = 3;

/// How many bytes one pixel takes, in the readback and in the buffer alike.
///
/// [`Pixels`] states four, and every fourcc [`fourcc`] answers is a 32-bit format.
const BYTES_PER_PIXEL: usize = 4;

/// How many memory planes a framebuffer request can name.
///
/// `drm_mode_fb_cmd2` holds four handles, four strides and four offsets, and Vulkan defines four
/// memory-plane aspects. A layout claiming more than this is refused rather than truncated.
const PLANES: usize = 4;

/// The texture a frame this backend puts on a display is composed into.
///
/// Eight bits a channel and blue first. Unsigned normalised rather than sRGB, because a scanout
/// hands the bytes to the display as they are and a second encoding would lighten every frame.
///
/// A renderer drawing for this backend asks its graphics device for this format. The two ends have
/// to agree, and they agree through this one name: the buffers a display scans out of are allocated
/// with the fourcc this format's channel order picks.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// Whether [`FORMAT`] stores its channels blue first.
///
/// [`Scanout::copied`] is given this when the frame loop makes the buffers. It is derived from the
/// format rather than stated a second time: a frame read back in the other order reaches the screen
/// with its red and blue exchanged, and nothing at all reports it.
pub(crate) const BGRA: bool = matches!(
    FORMAT,
    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
);

// Checked where the decision is made rather than in a test, because it is the sort of mistake that
// compiles: a `FORMAT` changed to a red-first texture leaves every frame on the screen with its red
// and blue exchanged, and nothing at all reports it.
const _: () = assert!(
    BGRA,
    "a scanout copies a frame rather than swizzling it, so the texture has to be blue first"
);

/// The buffers one display is driven from, and the flip that swaps them.
#[derive(Debug)]
pub struct Scanout {
    /// The connector, the CRTC and the plane a commit names.
    pipe: Pipe,
    /// The mode the first present sets.
    mode: Mode,
    /// The buffers, in one of the two shapes.
    buffers: Buffers,
    /// What each buffer is doing, and therefore which one a frame goes into.
    ///
    /// It also holds the frame that is waiting for an outstanding flip, and the fence that frame is
    /// committed with. `rotation` is where every decision it takes is set out.
    rotation: Rotation<OwnedFd>,
    /// Whether the mode has been set. The first present sets it.
    lit: bool,
}

/// The buffers a display is driven from, in one of the two shapes.
///
/// Private: which shape a display took is answered by [`Scanout::buffers`] and
/// [`Scanout::acquire`], and what a caller does about it is the same either way.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "the difference is a Vulkan dispatch table, which `Handover` holds by value because \
              that is what `ash::Device` is. There is one of these per display and it is moved \
              once, when the display is set up, so boxing it would buy an allocation and an \
              indirection on the frame path in exchange for nothing"
)]
enum Buffers {
    /// Buffers the driver allocated, which a frame is copied into.
    Copied {
        /// The buffers, in the order they were allocated.
        buffers: [DumbBuffer; COPIED],
        /// The framebuffer each buffer is registered as, at the same index.
        framebuffers: [Framebuffer; COPIED],
    },
    /// Images the renderer draws into, which the display engine reads where they lie.
    Imported {
        /// The pair of barriers that passes an image between the renderer and the display engine.
        ///
        /// Declared before the buffers so that it is dropped before them: it holds a raw device
        /// handle, which keeps nothing alive, and the buffers hold the textures that do.
        handover: Handover,
        /// The images, in the order they were made.
        buffers: Vec<Imported>,
        /// The GEM handle each image's descriptor imported as, at the same index.
        handles: Vec<ImportedBuffer>,
        /// The framebuffer each image is registered as, at the same index.
        framebuffers: Vec<Framebuffer>,
    },
}

impl Scanout {
    /// Builds the buffers `output` is driven from, importing them where both ends allow it.
    ///
    /// This is the one place the choice is made, and it says in the log which shape the display
    /// took and why. A display that cannot take the imported shape is driven from the copied one,
    /// which every machine has.
    ///
    /// `pointer_on_a_plane` is [`Cursor::on_a_plane`]. `gpu` is the device the renderer will draw
    /// on, and `bgra` is the channel order a readback produces — see [`Scanout::copied`] for what
    /// that one picks.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the copied shape itself cannot be built, which is a
    /// driver that refuses a buffer or a framebuffer. A display that cannot take the imported shape
    /// is answered with the copied one rather than with an error.
    pub fn for_display(
        device: &Device,
        output: &Output,
        gpu: &Gpu,
        pointer_on_a_plane: bool,
        bgra: bool,
    ) -> Result<Self, PlatformError> {
        match Self::imported(device, output, gpu, pointer_on_a_plane) {
            Ok(scanout) => {
                info!(
                    crtc = output.pipe.crtc,
                    "the renderer draws straight into the buffers this display scans out"
                );
                Ok(scanout)
            }
            Err(reason) => {
                info!(
                    crtc = output.pipe.crtc,
                    "every frame for this display is copied into a buffer the driver allocated, \
                     because {reason}"
                );
                Self::copied(device, output, bgra)
            }
        }
    }

    /// Allocates two buffers for `output`, both registered, with the mode still unset.
    ///
    /// `bgra` says which order the frames handed to [`Scanout::present`] store their channels,
    /// which [`Pixels::is_bgra`] answers. It picks the fourcc: `XRGB8888` for bytes that are blue
    /// first, `XBGR8888` for bytes that are red first. Choosing the format leaves a frame a copy
    /// rather than a swizzle of two million pixels.
    ///
    /// Nothing reaches the screen here. The first [`Scanout::present`] sets the mode, so a display
    /// keeps whatever the console left on it until there is a frame to replace it with.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses a buffer or a framebuffer.
    /// Whatever was allocated before the refusal is released, so a failure here leaves the device
    /// as it was found.
    pub fn copied(device: &Device, output: &Output, bgra: bool) -> Result<Self, PlatformError> {
        let format = fourcc(bgra);
        let width = output.mode.width();
        let height = output.mode.height();

        let (front, front_id) = allocate(device, width, height, format)?;
        let (back, back_id) = match allocate(device, width, height, format) {
            Ok(pair) => pair,
            Err(error) => {
                drop(device.remove_framebuffer(front_id));
                drop(device.destroy_dumb_buffer(front));
                return Err(error);
            }
        };

        Ok(Self::new(
            output,
            Buffers::Copied {
                buffers: [front, back],
                framebuffers: [front_id, back_id],
            },
        ))
    }

    /// Creates three images for `output` that the renderer draws into and the display reads where
    /// they lie.
    ///
    /// Each image is created in a layout the display's own plane published, exported as a dma-buf,
    /// imported as a GEM handle and registered as a framebuffer carrying that layout and every
    /// memory plane's offset and stride. The barrier that hands a drawn image over is recorded here
    /// too, once per image.
    ///
    /// `pointer_on_a_plane` is [`Cursor::on_a_plane`]'s own answer. A display without one is
    /// refused: the pointer there is drawn into the frame by the processor, and a tiled image is
    /// not something the processor can draw into.
    ///
    /// # Errors
    ///
    /// Returns [`Copied`], which names which of four reasons the display keeps the copied shape
    /// for. Whatever was taken before a refusal is given back, so a display that cannot do this
    /// pays nothing for having tried.
    pub fn imported(
        device: &Device,
        output: &Output,
        gpu: &Gpu,
        pointer_on_a_plane: bool,
    ) -> Result<Self, Copied> {
        if !pointer_on_a_plane {
            return Err(Copied::NoCursorPlane);
        }
        // The fourcc is the one the images are created in: `Imported` states `B8G8R8A8_UNORM`, and
        // `XRGB8888` is that byte order. A display that cannot scan it out cannot be imported for.
        let format = fourcc(BGRA);
        let layouts = published(device, output.pipe.plane, format)?;
        let width = output.mode.width();
        let height = output.mode.height();

        // Whether this display can be told to wait for the graphics device rather than this
        // program waiting for it. A plane that cannot be read is answered as one that cannot, and
        // not as a display that has to be copied: what it costs is the wait, and the display works
        // either way.
        let fenced = waits_for_a_fence(device, output.pipe.plane).unwrap_or_else(|error| {
            warn!(
                plane = output.pipe.plane,
                "this plane's properties could not be read, so every frame for this display is \
                 waited for before it is committed: {error}"
            );
            false
        });

        let buffers =
            Imported::create(gpu, &layouts, width, height, IMPORTED).map_err(Copied::NoImages)?;
        let handover = Handover::record(gpu, &buffers, fenced).map_err(Copied::NoImages)?;

        // From here on the kernel holds handles and framebuffers of ours, so a refusal part way
        // through has to give them back. The guard does that, and taking it apart at the end
        // stops it.
        let mut registering = Registering {
            device,
            handles: Vec::with_capacity(buffers.len()),
            framebuffers: Vec::with_capacity(buffers.len()),
        };
        for (slot, buffer) in buffers.iter().enumerate() {
            let imported = device.import_buffer(buffer.dmabuf()).map_err(|error| {
                Copied::Refused(format!("buffer {slot} did not import: {error}"))
            })?;
            // Handed to the guard before anything else can refuse, because from here on the kernel
            // holds a handle that this is the only owner of.
            let handle = imported.handle();
            registering.handles.push(imported);

            let planes = layout(handle, buffer.layouts()).ok_or_else(|| {
                Copied::Refused(format!(
                    "buffer {slot} is laid out in {} memory plane(s), and a framebuffer states 1 \
                     to {PLANES}",
                    buffer.layouts().len()
                ))
            })?;
            let framebuffer = device
                .add_framebuffer_from_handles(
                    width,
                    height,
                    format,
                    planes.handles,
                    planes.strides,
                    planes.offsets,
                    Some(buffer.modifier()),
                )
                .map_err(|error| {
                    Copied::Refused(format!(
                        "the kernel refused a framebuffer over buffer {slot} in {:#018x}: {error}",
                        buffer.modifier().0
                    ))
                })?;
            registering.framebuffers.push(framebuffer);
        }
        let (handles, framebuffers) = registering.take();

        Ok(Self::new(
            output,
            Buffers::Imported {
                handover,
                buffers,
                handles,
                framebuffers,
            },
        ))
    }

    /// Returns the images the renderer composes into, in the order they were made.
    ///
    /// What `SharedGraphics::renderer_supplied` is given, one texture out of each. Empty on the
    /// copied shape, where a frame is composed into the renderer's own target and copied in
    /// afterwards.
    pub fn buffers(&self) -> &[Imported] {
        match &self.buffers {
            Buffers::Copied { .. } => &[],
            Buffers::Imported { buffers, .. } => buffers,
        }
    }

    /// Returns what the kernel knows this display's buffers by, in the order they were registered.
    ///
    /// A caller reports them. A framebuffer id came back from a `drm_mode_fb_cmd2`, so whether
    /// these exist at all tells a display showing nothing apart from a display the kernel never
    /// took the buffers of.
    pub fn framebuffers(&self) -> Vec<Framebuffer> {
        match &self.buffers {
            Buffers::Copied { framebuffers, .. } => framebuffers.to_vec(),
            Buffers::Imported { framebuffers, .. } => framebuffers.clone(),
        }
    }

    /// Takes the buffer the next frame is drawn into back from the display engine, and names it.
    ///
    /// The imported shape calls this, and it is the **only** way to learn which buffer to draw
    /// into: a renderer composing straight into a scanout buffer has to be told which one before it
    /// draws, and taking that buffer back has to happen before it draws as well. Answering the two
    /// together stops a caller doing one and forgetting the other. `WgpuRenderer::present_into` is
    /// where the answer goes.
    ///
    /// A flip on its way leaves a buffer free here, and that is what three buffers are for: one is
    /// on the screen, one is in the flip, and this answers the third. So a frame that arrives while
    /// a flip is outstanding is drawn at once rather than waiting for the vertical blank to begin.
    ///
    /// Answers nothing when every buffer is the display's, which is the buffer on the screen, the
    /// buffer the outstanding flip names, and a buffer holding a finished frame that waits for that
    /// flip. Nothing is taken back in that case, so nothing is owed back.
    ///
    /// Answers nothing on the copied shape as well. There a frame is composed into the renderer's
    /// own target, so this would name a buffer no caller draws into.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the graphics device refuses or does not finish the
    /// barrier that takes the buffer back.
    pub fn acquire(&mut self) -> Result<Option<usize>, PlatformError> {
        let Buffers::Imported { handover, .. } = &mut self.buffers else {
            return Ok(None);
        };
        let Some(slot) = self.rotation.drawing() else {
            return Ok(None);
        };
        handover
            .acquire(slot)
            .map_err(|refusal| PlatformError::Backend(refusal.to_string()))?;
        Ok(Some(slot))
    }

    /// Copies `pixels` into the back buffer, draws `cursor` over it, and shows it.
    ///
    /// The copied shape only. An imported display is presented to with
    /// [`Scanout::present_drawn`], because there the frame is already in the buffer.
    ///
    /// Answers `false` when no buffer is free, which on two buffers is a flip still on its way:
    /// one buffer is on the screen and the other is in the flip, so writing into either would tear
    /// and the caller's frame is declined rather than shown torn. [`Scanout::drain`] frees one
    /// again.
    ///
    /// `cursor` draws nothing where the display has a plane to put a pointer on, so passing it
    /// always keeps the decision in one place. It is drawn after the frame and before the flip,
    /// because a pointer under the picture is a pointer nobody sees.
    ///
    /// The first call sets the mode, which returns once the frame is on the screen. Every later
    /// one flips and returns at once; what says that frame arrived is the completion event, which
    /// the loop reads off the device and hands back through [`Scanout::drain`].
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when this display is driven from imported buffers, when
    /// `pixels` is not the extent of the buffers, when the buffer cannot be mapped, and when the
    /// driver refuses the mode or the flip.
    pub fn present(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        pixels: &Pixels,
        cursor: &Cursor,
    ) -> Result<bool, PlatformError> {
        let Buffers::Copied { buffers, .. } = &mut self.buffers else {
            return Err(PlatformError::Backend(
                "this display is driven from buffers the renderer draws into, so a frame reaches \
                 it through Scanout::present_drawn rather than through a copy"
                    .to_owned(),
            ));
        };
        let Some(slot) = self.rotation.drawing() else {
            return Ok(false);
        };

        let width = buffers[slot].width();
        let height = buffers[slot].height();
        let size = pixels.size();
        if u32::try_from(size.width) != Ok(width) || u32::try_from(size.height) != Ok(height) {
            return Err(PlatformError::Backend(format!(
                "a frame of {}x{} cannot be scanned out of a {width}x{height} buffer",
                size.width, size.height
            )));
        }

        // The driver rounds a row up, so the two strides differ and each side steps by its own.
        let destination_stride = buffers[slot].stride() as usize;
        let source_stride = width as usize * BYTES_PER_PIXEL;
        let bytes = buffers[slot].bytes(device).map_err(backend)?;
        blit(
            pixels.bytes(),
            source_stride,
            bytes,
            destination_stride,
            height as usize,
        );
        cursor.draw(bytes, destination_stride, width, height);

        // No fence: the processor wrote the picture into this buffer, so it is already there.
        let Some(ready) = self.rotation.finished(slot, None) else {
            // Two buffers reach this on none of their paths: a held frame, the buffer on the screen
            // and the buffer in the flip are three. A shape with more would hold the frame here and
            // `Scanout::drain` would put it up, which is why nothing is refused.
            return Ok(true);
        };
        self.show(device, commit, ready)?;
        Ok(true)
    }

    /// Gives the image the renderer just drew into to the display engine, and shows it.
    ///
    /// The imported shape only. The frame is already in the buffer [`Scanout::acquire`] named, so
    /// nothing is copied here. What runs is the barrier that moves the image to
    /// `VK_IMAGE_LAYOUT_GENERAL` and gives it to `VK_QUEUE_FAMILY_FOREIGN_EXT`, so what the frame
    /// drew becomes what the display engine reads.
    ///
    /// The barrier is submitted on the queue the frame was submitted on and is therefore ordered
    /// after it. What waits for it is the **kernel**, wherever it can: the barrier signals a
    /// semaphore exported as a sync file, the commit is handed that descriptor as the plane's
    /// `IN_FENCE_FD`, and the display engine reads the buffer once it signals. So this returns
    /// without blocking on the graphics device at all.
    ///
    /// Where the kernel cannot be told — a display on the legacy interface, a plane with no
    /// `IN_FENCE_FD` property, a graphics driver that exports no sync file — this blocks until the
    /// barrier has run and then commits, which is the only other place the wait can happen.
    ///
    /// **A frame that finishes while a flip is on its way is held rather than declined.** The
    /// kernel takes one page flip per CRTC, so it cannot be committed yet; it goes to the driver
    /// when the completion arrives, in the buffer it was drawn into and with the fence its own
    /// drawing signals. [`Scanout::drain`] puts it up. Either way this answers `true`: the frame's
    /// work is done and it reaches the screen.
    ///
    /// Answers `false` for a display no frame was drawn on, which is one whose
    /// [`Scanout::acquire`] answered nothing. Nothing is submitted in that case.
    ///
    /// The pointer is not drawn: an imported display has one on a plane, and [`Scanout::imported`]
    /// refuses a display without one.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when this display is driven from copied buffers, when
    /// the buffer was never taken back from the display engine, when the graphics device refuses
    /// or does not finish the barrier, and when the driver refuses the mode or the flip.
    pub fn present_drawn(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
    ) -> Result<bool, PlatformError> {
        let Buffers::Imported { handover, .. } = &mut self.buffers else {
            return Err(PlatformError::Backend(
                "this display is driven from buffers the processor writes, so a frame reaches it \
                 through Scanout::present rather than by being drawn into it"
                    .to_owned(),
            ));
        };
        let Some(slot) = self.rotation.drawn() else {
            return Ok(false);
        };

        let fence = handover
            .release(slot)
            .map_err(|refusal| PlatformError::Backend(refusal.to_string()))?;

        let Some(ready) = self.rotation.finished(slot, fence) else {
            return Ok(true);
        };
        self.show(device, commit, ready)?;
        Ok(true)
    }

    /// Reads this display's completion out of `events`, and shows the frame that waited for it.
    ///
    /// The loop reads the device once and hands every scanout the same slice, because one read
    /// carries the completions of every CRTC that finished. An event naming another CRTC is
    /// another display's, and this leaves it alone.
    ///
    /// The buffer the flip named is the one the display reads from now, and the buffer it was
    /// reading is free. **A frame held while that flip was on its way is committed here**, with its
    /// own fence, and this is the only place it can be: the kernel takes one page flip per CRTC, so
    /// the next one becomes legal when the completion arrives.
    ///
    /// So this takes the device the display is on and the commit every flip on that device goes
    /// through. A caller with neither has nothing this can do.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses the held frame's flip. The buffer
    /// is left where the next frame draws over it, so a display recovers at the frame after this
    /// one.
    pub fn drain(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        events: &[Event],
    ) -> Result<(), PlatformError> {
        if !completed(events, self.pipe.crtc) {
            return Ok(());
        }
        let Some(ready) = self.rotation.completed() else {
            return Ok(());
        };
        self.show(device, commit, ready)
    }

    /// Gives the framebuffers and the buffers back.
    ///
    /// Taken by value, because everything it holds is dead afterwards. A refusal is reported
    /// through the log rather than returned: this runs while a program is shutting down, where
    /// there is nothing a caller could do with it, and the rest still has to be released.
    ///
    /// The framebuffers go first. Removing one that an enabled plane is scanning out disables that
    /// plane, so the display goes dark on shutdown.
    pub fn release(self, device: &Device) {
        for framebuffer in self.framebuffers() {
            if let Err(error) = device.remove_framebuffer(framebuffer) {
                let id = framebuffer.id();
                warn!(
                    "framebuffer {id} could not be removed, so it stays until the device \
                     closes: {error}"
                );
            }
        }
        match self.buffers {
            Buffers::Copied { buffers, .. } => {
                for buffer in buffers {
                    if let Err(error) = device.destroy_dumb_buffer(buffer) {
                        warn!("a scanout buffer could not be released: {error}");
                    }
                }
            }
            Buffers::Imported {
                handover,
                buffers,
                handles,
                ..
            } => {
                // The barriers first: they reach the graphics device through a handle that keeps
                // nothing alive, and the textures below are what keeps that device open.
                drop(handover);
                // One release per handle. The kernel counts no references for a GEM handle, and
                // every image here is its own allocation, so every import answered with a handle
                // of its own.
                for handle in handles {
                    if let Err(error) = device.release_imported(handle) {
                        warn!("an imported scanout buffer could not be released: {error}");
                    }
                }
                // The descriptors close here, and wgpu destroys each image and frees its memory as
                // it reaches the texture.
                drop(buffers);
            }
        }
    }

    /// Creates a scanout for `output` holding `buffers`, with nothing on the screen yet.
    fn new(output: &Output, buffers: Buffers) -> Self {
        // Nothing is on the screen, so every buffer is free: the first frame goes into the first of
        // them and the modeset puts that one up.
        let rotation = Rotation::new(match &buffers {
            Buffers::Copied { buffers, .. } => buffers.len(),
            Buffers::Imported { buffers, .. } => buffers.len(),
        });
        Self {
            pipe: output.pipe,
            mode: output.mode,
            buffers,
            rotation,
            lit: false,
        }
    }

    /// Returns the framebuffer the kernel knows the buffer at `slot` by.
    ///
    /// `slot` is one the rotation answered, so it names a buffer of this display.
    fn framebuffer(&self, slot: usize) -> Framebuffer {
        match &self.buffers {
            Buffers::Copied { framebuffers, .. } => framebuffers[slot],
            Buffers::Imported { framebuffers, .. } => framebuffers[slot],
        }
    }

    /// Puts the frame `ready` names on the screen, setting the mode the first time.
    ///
    /// `ready` carries the buffer and the sync file the display engine waits for before it reads
    /// that buffer, so a frame reaches the driver with its own fence and never with the one of
    /// whatever flipped last. Both halves carry it: the first frame of a display arrives through
    /// the modeset, so a fence left out there would put the one frame the graphics device had not
    /// finished on the screen.
    ///
    /// **The descriptor closes here**, on a commit the driver took and on one it refused alike.
    /// [`Commit::flip`] states why: the kernel reads the fence out of the sync file and gives the
    /// file back, so a caller that expected it to be taken leaks one per frame — and a refused
    /// frame is exactly as often as a shown one on a machine that is not DRM master.
    ///
    /// A modeset carries no completion event — the call returning says the frame is up — so
    /// nothing is left outstanding by the first one and the second present is taken at once. A flip
    /// is the other way round, and the rotation holds the next frame off until the device reports
    /// it.
    fn show(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        ready: Ready<OwnedFd>,
    ) -> Result<(), PlatformError> {
        let slot = ready.slot;
        let framebuffer = self.framebuffer(slot);
        let fence = ready.fence.as_ref().map(AsFd::as_fd);
        let flipping = self.lit;
        let taken = if flipping {
            commit.flip(device, self.pipe, framebuffer, fence)
        } else {
            commit.modeset(device, self.pipe, &self.mode, framebuffer, fence)
        };
        drop(ready.fence);
        taken.map_err(backend)?;

        // After the commit, so that what the rotation records is a frame the driver took. A refusal
        // leaves the buffer where the next frame draws over it.
        if flipping {
            self.rotation.flipped(slot);
        } else {
            self.lit = true;
            self.rotation.shown(slot);
        }
        Ok(())
    }

    /// Puts this display back into its mode, showing the frame it last showed.
    ///
    /// What a session that has been away calls. Another session has set its own mode on the CRTC
    /// and taken the plane, so the way back is a modeset rather than a flip — the atomic commit
    /// carries `ALLOW_MODESET`, and the legacy one sets the CRTC. The buffer is this process's own
    /// and still holds the last frame it drew, so the picture is back when this returns.
    ///
    /// Answers whether anything was put back. A display that never presented has nothing: nothing
    /// is committed for it, and the first present after this sets the mode with the buffer it would
    /// have used at start-up. That is a run started on a terminal nobody was looking at.
    ///
    /// **A frame that was held goes up rather than the last one committed**, with its own fence. It
    /// is the newest picture this program drew and its buffer still holds it, so putting the one
    /// before it up would show a picture this program has already replaced.
    ///
    /// **Any flip from before is forgotten.** Its completion is not coming — the frame it named was
    /// on a CRTC another session then took — and a display that kept waiting for one would decline
    /// every frame for the rest of the program.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses the mode. The display is left
    /// unlit then, so the next present sets it again rather than flipping onto a CRTC that is
    /// showing nothing of this program's.
    pub fn restore(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
    ) -> Result<bool, PlatformError> {
        let Some(ready) = self.rotation.restores() else {
            return Ok(false);
        };
        self.lit = false;

        self.show(device, commit, ready)?;
        Ok(true)
    }
}

/// Why a display is driven through the copied shape.
///
/// Every one of these is an ordinary fact about a machine or a display rather than a fault, and the
/// answer to all four is the same: copy every frame. They are told apart because the remedies
/// differ — a display with no cursor plane is hardware, and a graphics device that cannot export an
/// image may be a program that asked for the wrong extensions.
///
/// ```
/// use zgui_platform_drm::Copied;
///
/// let reason = Copied::NoCursorPlane;
///
/// assert!(
///     reason.to_string().contains("composites no pointer"),
///     "a reason states itself, so a caller logs it as one line"
/// );
/// assert!(std::error::Error::source(&reason).is_none());
/// ```
#[derive(Debug)]
pub enum Copied {
    /// The display engine composites no pointer, so the frames have to carry it.
    ///
    /// A pointer is drawn into the frame by the processor, and the imported buffers are tiled
    /// images the processor cannot address. So this display keeps the copied shape whatever else
    /// it could do.
    NoCursorPlane,
    /// The display's own plane names no layout for the fourcc a scanout uses.
    ///
    /// A driver that publishes no `IN_FORMATS` property is the ordinary case, and it is a driver
    /// saying only which formats it takes. Without a layout list there is nothing to agree with
    /// the graphics device about.
    NoLayouts(String),
    /// The graphics device cannot make the images.
    ///
    /// [`Unsupported`] says which of its own four reasons it was.
    NoImages(Unsupported),
    /// The kernel refused a descriptor or a framebuffer.
    ///
    /// The images exist and the display cannot be given them: memory this driver has no path to,
    /// or a layout it publishes and will not take in a framebuffer.
    Refused(String),
}

impl fmt::Display for Copied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCursorPlane => write!(
                formatter,
                "this display composites no pointer of its own, so its frames have to carry one \
                 and the processor has to be able to draw into them"
            ),
            Self::NoLayouts(reason) => write!(formatter, "{reason}"),
            Self::NoImages(refusal) => write!(formatter, "{refusal}"),
            Self::Refused(reason) => write!(formatter, "{reason}"),
        }
    }
}

/// A reason reads as an error wherever one is expected, which is how a caller logs it.
impl std::error::Error for Copied {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoImages(refusal) => Some(refusal),
            _ => None,
        }
    }
}

/// Returns the layouts `plane` can scan `format` out in.
///
/// # Errors
///
/// Returns [`Copied::NoLayouts`] when the plane cannot be read, when it publishes no `IN_FORMATS`
/// property, and when it names no layout for this fourcc. All three say the same thing: there is
/// nothing here to agree with the graphics device about.
fn published(device: &Device, plane: u32, format: Format) -> Result<Vec<Modifier>, Copied> {
    let published = device
        .plane_formats(plane)
        .map_err(|error| Copied::NoLayouts(format!("plane {plane} could not be read: {error}")))?
        .ok_or_else(|| {
            Copied::NoLayouts(format!("plane {plane} publishes no IN_FORMATS property"))
        })?;
    let layouts = published.modifiers(format).to_vec();
    if layouts.is_empty() {
        return Err(Copied::NoLayouts(format!(
            "plane {plane} names no layout it can scan {format:?} out in"
        )));
    }
    Ok(layouts)
}

/// The handles, strides and offsets a framebuffer request takes, filled from one image's layout.
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    /// One handle per memory plane, and zero past the end.
    handles: [u32; PLANES],
    /// One stride per memory plane, and zero past the end.
    strides: [u32; PLANES],
    /// One offset per memory plane, and zero past the end.
    offsets: [u32; PLANES],
}

/// Returns the four slots a framebuffer request takes, filled from `planes` of one image.
///
/// Every memory plane names the same handle, because they are regions of one allocation and that
/// allocation imported as one buffer. What tells them apart is the offset, and the kernel requires
/// the entries past the last plane to be zero.
///
/// Answers nothing for a layout with no memory plane and for one with more than a request can hold.
/// Neither can be truncated into something true: a layout with no plane addresses no pixels, and a
/// layout with five would leave one unaddressed.
fn layout(handle: u32, planes: &[Plane]) -> Option<Layout> {
    if planes.is_empty() || planes.len() > PLANES {
        return None;
    }
    let mut filled = Layout {
        handles: [0; PLANES],
        strides: [0; PLANES],
        offsets: [0; PLANES],
    };
    for (index, plane) in planes.iter().enumerate() {
        filled.handles[index] = handle;
        filled.strides[index] = plane.stride();
        filled.offsets[index] = plane.offset();
    }
    Some(filled)
}

/// The handles and framebuffers of an imported set while it is still being registered.
///
/// Gives both back when it is dropped, so a kernel that refuses one buffer of three leaves the
/// device holding none of them. A finished set stops it with [`Registering::take`].
struct Registering<'a> {
    /// The device both belong to.
    device: &'a Device,
    /// The handles taken so far.
    handles: Vec<ImportedBuffer>,
    /// The framebuffers registered so far.
    framebuffers: Vec<Framebuffer>,
}

impl Registering<'_> {
    /// Returns the handles and the framebuffers, with this guard disarmed.
    fn take(mut self) -> (Vec<ImportedBuffer>, Vec<Framebuffer>) {
        (
            std::mem::take(&mut self.handles),
            std::mem::take(&mut self.framebuffers),
        )
    }
}

impl Drop for Registering<'_> {
    fn drop(&mut self) {
        // The framebuffers first, for the reason `Scanout::release` gives.
        for framebuffer in self.framebuffers.drain(..) {
            drop(self.device.remove_framebuffer(framebuffer));
        }
        for handle in self.handles.drain(..) {
            drop(self.device.release_imported(handle));
        }
    }
}

/// Returns the fourcc whose bytes lie in the order the readback produced.
///
/// A fourcc names its channels most significant first inside one 32-bit word, and `drm_fourcc.h`
/// writes that out: `XRGB8888` is "[31:0] x:R:G:B 8:8:8:8 little endian". Little endian puts the
/// word's least significant byte first in memory, so `x:R:G:B` reaches memory as **B, G, R, x** —
/// which is a BGRA readback. `XBGR8888` is "[31:0] x:B:G:R", so **R, G, B, x** in memory, which is
/// an RGBA one.
///
/// Choosing here rather than swizzling is the point: a swizzle over a 1920x1080 frame is two
/// million operations a frame to reach a format the display could have been given directly.
///
/// The `X` form rather than the `A` form: the scanout ignores that byte, so a frame whose alpha is
/// anything other than opaque still reaches the screen as it was drawn.
fn fourcc(bgra: bool) -> Format {
    if bgra {
        Format::XRGB8888
    } else {
        Format::XBGR8888
    }
}

/// Copies `rows` rows from `source` into `destination`, each stepping by its own stride.
///
/// The source is tightly packed, so its stride is the row's own width in bytes. The destination is
/// the driver's buffer, whose stride is rounded up past that, and the bytes past the end of a row
/// are padding this leaves alone. A copy that stepped both by the same number would write a
/// diagonal.
///
/// **A short side truncates.** Whichever of the two runs out first ends the copy, and a row with
/// too little room is left out whole. So this can put part of a picture on a screen, and it can
/// panic on no input at all. [`Scanout::present`] refuses a frame of the wrong extent before it
/// reaches here; this one stays total, so that a mistake above it is a fault a person can see
/// rather than a crash inside a frame loop. A stride of zero copies nothing, for the same reason.
fn blit(
    source: &[u8],
    source_stride: usize,
    destination: &mut [u8],
    destination_stride: usize,
    rows: usize,
) {
    if source_stride == 0 || destination_stride == 0 {
        return;
    }
    // A destination narrower than the source is the same truncation one row along.
    let width = source_stride.min(destination_stride);
    for (into, from) in destination
        .chunks_exact_mut(destination_stride)
        .zip(source.chunks_exact(source_stride))
        .take(rows)
    {
        into[..width].copy_from_slice(&from[..width]);
    }
}

/// Returns `true` if any of `events` says a flip on `crtc` finished.
fn completed(events: &[Event], crtc: u32) -> bool {
    events.iter().any(
        |event| matches!(event, Event::FlipComplete { crtc: finished, .. } if *finished == crtc),
    )
}

/// Allocates one buffer of this extent, registered for scanout.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when the driver refuses either half. A buffer whose
/// framebuffer was refused is released here rather than left allocated until the device closes.
fn allocate(
    device: &Device,
    width: u32,
    height: u32,
    format: Format,
) -> Result<(DumbBuffer, Framebuffer), PlatformError> {
    let buffer = device
        .create_dumb_buffer(width, height, format)
        .map_err(backend)?;
    match device.add_framebuffer(&buffer, format) {
        Ok(framebuffer) => Ok((buffer, framebuffer)),
        Err(error) => {
            drop(device.destroy_dumb_buffer(buffer));
            Err(backend(error))
        }
    }
}

#[cfg(test)]
mod tests {
    //! The decisions a device cannot help with: which fourcc a readback is, the copy, and the four
    //! slots a framebuffer request is filled from.
    //!
    //! The copy is where a display shows a diagonal, and it needs no hardware to prove: the source
    //! and the destination are slices, and the strides differ here exactly as they differ on a
    //! driver that rounds a row up. The slots are where an imported buffer reaches the kernel
    //! wrong, and they are four arrays.

    use super::{Layout, PLANES, blit, completed, fourcc, layout};
    use crate::import::Plane;
    use std::time::Duration;
    use zgui_drm::Event;
    use zgui_drm::format::Format;

    /// A destination of `rows` rows of `stride` bytes, filled with a byte no source writes.
    fn padded(stride: usize, rows: usize) -> Vec<u8> {
        vec![0xAA; stride * rows]
    }

    #[test]
    fn a_bgra_readback_is_scanned_out_as_the_fourcc_whose_bytes_are_blue_first() {
        assert_eq!(
            fourcc(true),
            Format::XRGB8888,
            "x:R:G:B is B, G, R, x in memory"
        );
        assert_eq!(fourcc(false), Format::XBGR8888, "and x:B:G:R is R, G, B, x");
    }

    #[test]
    fn a_source_narrower_than_the_destination_lands_one_row_per_row() {
        let source = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut destination = padded(6, 2);

        blit(&source, 4, &mut destination, 6, 2);

        assert_eq!(
            destination,
            [1, 2, 3, 4, 0xAA, 0xAA, 5, 6, 7, 8, 0xAA, 0xAA],
            "every row starts at its own stride, and the padding is left alone"
        );
    }

    #[test]
    fn two_equal_strides_copy_the_whole_of_the_source() {
        let source = [1, 2, 3, 4, 5, 6];
        let mut destination = padded(3, 2);

        blit(&source, 3, &mut destination, 3, 2);

        assert_eq!(destination, source, "nothing is left over on either side");
    }

    #[test]
    fn no_rows_copies_nothing() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 4, &mut destination, 4, 0);

        assert_eq!(destination, [0xAA; 4], "a frame of no rows writes no bytes");
    }

    #[test]
    fn a_destination_too_short_takes_the_rows_that_fit_and_no_more() {
        // Three rows asked for, two rows of room. The alternative to truncating is an index past
        // the end, which in a frame loop is a panic per frame.
        let source = [1, 2, 3, 4, 5, 6];
        let mut destination = padded(4, 2);

        blit(&source, 2, &mut destination, 4, 3);

        assert_eq!(
            destination,
            [1, 2, 0xAA, 0xAA, 3, 4, 0xAA, 0xAA],
            "the rows that fit land whole, and the third is not written"
        );
    }

    #[test]
    fn a_source_too_short_supplies_the_rows_it_has() {
        let source = [1, 2, 3];
        let mut destination = padded(4, 3);

        blit(&source, 2, &mut destination, 4, 3);

        assert_eq!(
            destination,
            [
                1, 2, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA
            ],
            "one whole row is there, and the half row after it is not written"
        );
    }

    #[test]
    fn a_stride_of_zero_copies_nothing_rather_than_dividing_by_it() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 0, &mut destination, 4, 1);
        blit(&source, 4, &mut destination, 0, 1);

        assert_eq!(destination, [0xAA; 4]);
    }

    /// A completion for `crtc`, as the device reports one.
    fn flip(crtc: u32) -> Event {
        Event::FlipComplete {
            crtc,
            at: Duration::from_secs(1),
            user_data: 0,
        }
    }

    #[test]
    fn a_completion_naming_this_crtc_is_this_displays_flip() {
        assert!(completed(&[flip(62)], 62));
        assert!(
            completed(&[flip(81), flip(62)], 62),
            "one read carries the completions of every display that finished"
        );
    }

    #[test]
    fn a_completion_naming_another_crtc_belongs_to_another_display() {
        assert!(!completed(&[flip(81)], 62));
        assert!(
            !completed(&[], 62),
            "and a read with nothing in it says nothing"
        );
    }

    #[test]
    fn one_memory_plane_fills_the_first_slot_and_leaves_the_rest_at_zero() {
        // The measured layout: a 1920-wide image on this machine, one plane, rows 7680 apart.
        let filled = layout(7, &[Plane::of(0, 7680)]).expect("one plane fits a request");

        assert_eq!(
            filled,
            Layout {
                handles: [7, 0, 0, 0],
                strides: [7680, 0, 0, 0],
                offsets: [0, 0, 0, 0],
            },
            "the kernel reads a slot with no handle as a plane the image does not have"
        );
    }

    #[test]
    fn every_memory_plane_names_the_one_handle_and_its_own_offset() {
        // Three planes of one allocation, which is a layout with separate chroma planes.
        // The handle repeats because there is one buffer; what tells the planes apart is where
        // each starts.
        let filled = layout(
            9,
            &[
                Plane::of(0, 1920),
                Plane::of(2_073_600, 960),
                Plane::of(3_110_400, 960),
            ],
        )
        .expect("three planes fit a request");

        assert_eq!(
            filled,
            Layout {
                handles: [9, 9, 9, 0],
                strides: [1920, 960, 960, 0],
                offsets: [0, 2_073_600, 3_110_400, 0],
            }
        );
    }

    #[test]
    fn the_widest_layout_a_request_holds_still_fits() {
        let planes = [Plane::of(0, 1024); PLANES];

        let filled = layout(3, &planes).expect("four planes are what a request holds");

        assert_eq!(filled.handles, [3; PLANES], "every slot names the buffer");
    }

    #[test]
    fn a_layout_a_request_cannot_state_is_refused_rather_than_truncated() {
        assert_eq!(
            layout(3, &[]),
            None,
            "an image with no memory plane addresses no pixels"
        );
        assert_eq!(
            layout(3, &[Plane::of(0, 1024); PLANES + 1]),
            None,
            "and a fifth plane would reach the kernel as a plane the image does not have"
        );
    }
}
