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
//! follows. It is the answer for every machine where the other shape cannot be built.
//!
//! Only the rows that changed make that journey. A frame states which they are, and every buffer
//! it did not land in is told, so a buffer is copied the rows it owes rather than all of them.
//! Reading a whole frame back is what this shape costs; reading a hundred rows of one is what it
//! costs when a hundred rows changed.
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
pub(crate) mod watched;

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::time::{Duration, Instant};

use tracing::{info, warn};
use zgui_bits::DamageSet;
use zgui_drm::buffer::{DumbBuffer, ImportedBuffer};
use zgui_drm::commit::{Commit, Pipe, waits_for_a_fence};
use zgui_drm::format::{Format, Modifier};
use zgui_drm::framebuffer::Framebuffer;
use zgui_drm::resources::Mode;
use zgui_drm::{Device, Event};
use zgui_geom::{Device as DeviceSpace, Rect};
use zgui_platform::{PlatformError, Watchdog, refresh_interval};
use zgui_render_wgpu::{Gpu, Pixels, wgpu};

use crate::cursor::Cursor;
use crate::import::{Handover, Imported, Plane, Unsupported, gbm, gl};
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

/// How many buffers the drawn shape drives a display from.
///
/// One more than [`IMPORTED`], and the difference is [`rotation::Slot::Submitted`]: a frame here
/// goes to the graphics device and is still being drawn when the call returns, so its buffer is the
/// card's until the fence signals. The buffer the *next* frame is composed into therefore has to be
/// a fourth — none of the one on the screen, the one an outstanding flip names, and the one the
/// card is drawing.
///
/// **Only this shape ever has one.** The Vulkan shape hands its image to the display engine with a
/// fence the moment the frame is recorded, so nothing of its is ever outstanding and a fourth image
/// would be memory nothing reaches.
///
/// Measured on the slowest machine this runs on, against three: the graphics device goes from 74%
/// busy to 98%, and 27.6 frames a second become 32.5. Three does not merely lose the overlap — it
/// is *worse* than not pipelining at all, 26.7 a second, because a loop with nowhere to compose
/// presents on 57% of its turns instead of 99%, damage piles up across the turns that skip, and
/// every frame that does reach the screen covers half as much again: 463 875 pixels drawn against
/// 308 495.
///
/// The fourth was expected to cost presented pixels, because a frame copies the rectangles drawn
/// while its own texture was not the one being written and that set is one frame longer. It costs
/// none — 619 984 against 620 920 — because the frames it adds to the union are ones an animation
/// was going to touch anyway.
const DRAWN: usize = 4;

/// How many disjoint rectangles a scanout buffer's debt is tracked as.
///
/// A buffer is composited every [`DRAWN`] frames and owes what the frames in between drew, so the
/// set has to hold that many frames' damage before it starts merging. Below that the rectangles
/// merge, the merged ones cover the gaps between them, and the repair copies pixels no frame ever
/// changed — which on the display's own device is bandwidth rather than correctness, but there is
/// no reason to spend it.
///
/// **Not smaller as a way of making the arithmetic cheaper.** Eight was tried, and the sets then
/// merged into a single region that the donor covered as well, so nothing was ever repaired and
/// the picture kept pixels from four frames ago. What made the arithmetic cheap was removing it —
/// see [`Scanout::composite`], which subtracts nothing at all.
const STALE: usize = zgui_bits::MAX_DAMAGE * DRAWN;

/// How many buffers a frame is staged through on its way to the display.
///
/// **The renderer draws into these and never into a scanout buffer.** With one set of buffers
/// written by both devices, the graphics device's copy at the end of a frame waits for the display
/// device's copy at the start of it — measured at 5 to 11 ms a frame, against a copy that costs
/// 0.95. Splitting them means each buffer has one writer, and the only thing that waits is the
/// display device reading what the graphics device wrote, which is a wait it can afford: it has
/// nothing else to do until those pixels exist.
///
/// Two, and for the same reason there is more than one of anything else here: with one, the frame
/// after next waits for the display device to finish reading it. Two gives a whole frame of slack,
/// which is far more than the copy needs.
const STAGING: usize = 2;

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
    /// When the flip the device has not reported yet was taken, if one is outstanding.
    ///
    /// A completion is what frees the buffer behind it, so a completion that never arrives stops
    /// this display for good. [`Scanout::overdue`] is when waiting for it stops.
    owed_since: Option<Instant>,
    /// Per copied buffer: the rows written into some other buffer since this one was last written.
    ///
    /// A frame is copied into one buffer and the display rotates between several, so the rows a
    /// frame changed are owed to every buffer it did not land in. Without this a copy limited to
    /// what changed would show, on alternate frames, whatever the buffer held two frames ago.
    stale: Vec<Vec<Range<u32>>>,
}

/// The most bands a buffer's staleness is tracked as before it becomes the whole frame.
///
/// A frame changing scattered rows for many frames in a row would otherwise grow a list nothing
/// bounds. Copying the whole frame is the answer that is always right; this is only about which
/// one is cheaper.
const MOST_BANDS: usize = 16;

/// A frame the card is still drawing, and how this program learns that it has finished.
///
/// The buffer it was drawn into is its until it has been given over. Others may be outstanding
/// beside it, oldest first, and they are settled in that order: the fences signal in the order the
/// frames were submitted, and the pictures have to reach the screen in that order too.
#[derive(Debug)]
struct Outstanding {
    /// Which buffer it is in.
    slot: usize,
    /// When it was submitted, which is what bounds how long it may stay here.
    since: Instant,
    /// The descriptor that says when the card has finished this frame.
    ///
    /// Nothing here where the buffer answered none, and then the wait happens on the loop's own
    /// thread at the next acquire.
    watched: Option<watched::Watched>,
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
    /// Buffers the renderer draws into on a machine with no Vulkan.
    ///
    /// The same arrangement as [`Buffers::Imported`] reached from the other end: `libgbm` allocates
    /// on the display's own node and EGL imports each descriptor as a texture, rather than Vulkan
    /// creating an image and exporting it. There is no pair of barriers to record — GL has no
    /// queue families to pass an image between — so what stands in for the handover is one fence at
    /// the end of the frame.
    DrawnGl {
        /// The buffers, in the order they were made.
        buffers: Vec<gl::Drawn>,
        /// The GEM handle each buffer's descriptor imported as, at the same index.
        handles: Vec<ImportedBuffer>,
        /// The framebuffer each buffer is registered as, at the same index.
        framebuffers: Vec<Framebuffer>,
        /// How a finished frame is signalled, decided once when the buffers were made.
        signal: gl::Signal,
        /// Whether the commit takes a fence, which decides what a frame's descriptor is used for.
        ///
        /// Where it does, the flip carries it and the kernel waits. Where it does not — a legacy
        /// display, or a plane with no `IN_FENCE_FD` — the loop parks on it instead. Both leave the
        /// waiting to a `poll`, and neither blocks in the graphics driver.
        fenced: bool,
        /// The frames that were submitted and not yet given over, oldest first.
        ///
        /// Under [`gl::Signal::Kernel`], and under [`gl::Signal::Written`] on a display that takes
        /// a fence, the frame goes to the driver with its own descriptor and nothing is ever
        /// outstanding.
        submitted: VecDeque<Outstanding>,
        /// The buffers the renderer draws into, which are not the ones the display reads.
        ///
        /// [`STAGING`] says why they exist. A frame's damage is drawn here and the display's own
        /// device copies it onto a scanout buffer, so each buffer has exactly one writer and the
        /// two devices never contend for one.
        staging: Vec<gl::Drawn>,
        /// Which staging buffer the next frame is drawn into.
        stage: usize,
        /// What each scanout buffer lacks: the damage of every frame that landed elsewhere.
        ///
        /// The same bookkeeping the renderer used to keep about the buffers it drew into, moved to
        /// the side that now writes them. A buffer is brought up to date from the newest one before
        /// this frame's own damage goes on top.
        stale: Vec<DamageSet<STALE>>,
        /// What copies between these buffers on the display's own device, where one could be built.
        composite: Option<zgui_scanout::egl::Egl>,
        /// The buffer holding the newest complete picture, which every repair is copied from.
        ///
        /// The last one composited, and **not** whatever the rotation last put on the screen: a
        /// frame the rotation drops was still composited, so it is still the newest picture and
        /// still the right thing to copy from. Nothing until the first frame.
        newest: Option<usize>,
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
        // Vulkan first, then the same arrangement through GL, then the copy every machine can do.
        // The two drawn shapes are tried in that order rather than chosen by backend, so a device
        // that has both takes the one whose handover the kernel can wait on.
        let refused = match Self::imported(device, output, gpu, pointer_on_a_plane) {
            Ok(scanout) => {
                info!(
                    crtc = output.pipe.crtc,
                    "the renderer draws straight into the buffers this display scans out"
                );
                return Ok(scanout);
            }
            Err(reason) => reason,
        };
        match Self::drawn_gl(device, output, gpu, pointer_on_a_plane) {
            Ok(scanout) => {
                info!(
                    crtc = output.pipe.crtc,
                    "the renderer draws straight into the buffers this display scans out, through \
                     OpenGL"
                );
                return Ok(scanout);
            }
            Err(reason) => info!(
                crtc = output.pipe.crtc,
                "this display cannot be drawn into through OpenGL either, because {reason}"
            ),
        }
        info!(
            crtc = output.pipe.crtc,
            "every frame for this display is copied into a buffer the driver allocated, because \
             {refused}"
        );
        Self::copied(device, output, bgra)
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

    /// Returns a descriptor for every frame the card is still drawing, oldest first.
    ///
    /// **What a loop puts in its own wait set.** Each becomes readable when the card has finished
    /// the frame it belongs to, so a loop parked on them learns that a frame is ready to be shown
    /// where it is allowed to block — beside the card, the input devices and its own timers.
    /// Nothing else here has to be asked, and asking the graphics driver instead is what
    /// the `watched` module says the cost of is.
    ///
    /// Empty on every shape but the drawn one, and empty where the frames carry no descriptor.
    pub fn drawing(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        let submitted = match &self.buffers {
            Buffers::DrawnGl { submitted, .. } => Some(submitted),
            Buffers::Copied { .. } | Buffers::Imported { .. } => None,
        };
        submitted
            .into_iter()
            .flatten()
            .filter_map(|frame| Some(frame.watched.as_ref()?.descriptor()))
    }

    /// Whether this display builds its own frames out of what the renderer hands it.
    ///
    /// True on the drawn shape, where [`Scanout::textures`] answers staging buffers and the
    /// display's own device copies what a frame wrote onto the buffer it scans out of. False where
    /// the renderer composes straight into the buffers the display reads, and those rotate.
    pub fn composites_its_own_frames(&self) -> bool {
        matches!(self.buffers, Buffers::DrawnGl { .. })
    }

    /// Returns the images the renderer composes into, in the order they were made.
    ///
    /// What `SharedGraphics::renderer_supplied` is given, one texture out of each. Empty on the
    /// copied shape, where a frame is composed into the renderer's own target and copied in
    /// afterwards.
    pub fn buffers(&self) -> &[Imported] {
        match &self.buffers {
            Buffers::Copied { .. } | Buffers::DrawnGl { .. } => &[],
            Buffers::Imported { buffers, .. } => buffers,
        }
    }

    /// Returns the textures the renderer composes into, in the order they were made.
    ///
    /// What `SharedGraphics::renderer_supplied` is given. Empty on the copied shape, where a frame
    /// is composed into the renderer's own target and copied in afterwards — and **empty is the
    /// answer that decides the whole arrangement**, so a caller reads this rather than guessing
    /// from the machine.
    pub fn textures(&self) -> Vec<wgpu::Texture> {
        match &self.buffers {
            Buffers::Copied { .. } => Vec::new(),
            // The **staging** buffers, and not the ones the display reads. `STAGING` says why the
            // renderer never draws into a scanout buffer.
            Buffers::DrawnGl { staging, .. } => {
                staging.iter().map(|held| held.texture().clone()).collect()
            }
            Buffers::Imported { buffers, .. } => {
                buffers.iter().map(|held| held.texture().clone()).collect()
            }
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
            Buffers::DrawnGl { framebuffers, .. } | Buffers::Imported { framebuffers, .. } => {
                framebuffers.clone()
            }
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
    /// **The frame before this one is settled here**, which is what [`Scanout::settle`] describes:
    /// the previous frame's wait happens at the latest moment that still leaves a buffer to name.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the graphics device refuses or does not finish the
    /// barrier that takes the buffer back, and when the driver refuses the settled frame's flip.
    pub fn acquire(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        gpu: &Gpu,
    ) -> Result<Option<usize>, PlatformError> {
        if matches!(self.buffers, Buffers::Copied { .. }) {
            return Ok(None);
        }
        self.settle(device, commit, gpu)?;
        // A frame the card has not finished keeps the buffer it was drawn into — the rotation holds
        // it in `Slot::Submitted` and answers a free one instead, which is the whole of what lets
        // this frame be composed while that one is drawn. Where none is free nothing is taken back,
        // so nothing is owed back: the caller keeps its damage and asks again, and the frame's own
        // descriptor becoming readable is what brings it here.
        let Some(slot) = self.rotation.drawing() else {
            return Ok(None);
        };
        // Only the Vulkan shape has a barrier to run. GL has no queue families to pass an image
        // between, so there is nothing to take back — the buffer was never given away, and what
        // keeps the display engine off it is the rotation and the fence at the end of the frame.
        if let Buffers::Imported { handover, .. } = &mut self.buffers {
            handover
                .acquire(slot)
                .map_err(|refusal| PlatformError::Backend(refusal.to_string()))?;
        }
        // **What the caller is told is not the scanout slot.** On the drawn shape the renderer
        // composes into a staging buffer and the display's own device puts it on the scanout buffer
        // the rotation just answered — see `STAGING`. The scanout slot stays here, where the
        // rotation is already holding it, and what goes back is the buffer the renderer may write.
        //
        // They rotate for the reason the scanout buffers do: the display's device is still reading
        // the one the last frame used, and a renderer handed it back would wait for that read.
        if let Buffers::DrawnGl { stage, staging, .. } = &mut self.buffers {
            *stage = (*stage + 1) % staging.len().max(1);
            return Ok(Some(*stage));
        }
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
        changed: &[Range<u32>],
        cursor: &Cursor,
    ) -> Result<bool, PlatformError> {
        let Buffers::Copied { buffers, .. } = &mut self.buffers else {
            return Err(PlatformError::Backend(
                "this display is driven from buffers the renderer draws into, so a frame reaches \
                 it through Scanout::present_drawn rather than through a copy"
                    .to_owned(),
            ));
        };
        // Every buffer owes the rows this frame changed, recorded **before** the two ways out
        // below. A declined frame retires its damage all the same — the composed target holds it
        // and nothing will name those rows again — so a buffer told about them only on the paths
        // that copy would keep whatever it held there for ever.
        owe(&mut self.stale, changed, self.mode.height());

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

        // This buffer is about to hold everything it was owed. A pointer drawn into the picture
        // rather than onto a plane moves without changing a row, so such a display copies all of
        // it: the rows the pointer has just left are as stale as the ones it has reached, and
        // nothing above here knows where it was.
        let owed = if cursor.on_a_plane() {
            core::mem::take(&mut self.stale[slot])
        } else {
            self.stale[slot] = Vec::new();
            zgui_render_wgpu::frame::damage::every_row(height)
        };

        // The driver rounds a row up, so the two strides differ and each side steps by its own.
        let destination_stride = buffers[slot].stride() as usize;
        let source_stride = width as usize * BYTES_PER_PIXEL;
        let bytes = buffers[slot].bytes(device).map_err(backend)?;
        for band in &owed {
            blit(
                pixels.bytes(),
                source_stride,
                bytes,
                destination_stride,
                band.clone(),
            );
        }
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

    /// Builds this frame's picture in the scanout buffer at `slot`, on the display's own device.
    ///
    /// Two copies. **The repair** brings the buffer up to date from the one holding the newest
    /// picture — a buffer written every third frame lacks what the frames between drew, and that is
    /// what it owes. **The apply** puts this frame's own damage on top, out of the staging buffer
    /// the renderer drew into. Neither crosses the link: both buffers are the display's.
    ///
    /// Ordered by the kernel rather than by waiting. The staging buffer carries the graphics
    /// device's fence, so reading it here waits for that drawing on the *display's* side, where
    /// there is nothing else to do until those pixels exist. Nothing on this thread blocks.
    ///
    /// Does nothing where no device could be opened to composite with, which is a display that
    /// never took this shape.
    fn composite(&mut self, slot: usize, wrote: &[Rect<i32, DeviceSpace>]) {
        let whole = Rect::new(
            zgui_geom::Point::new(0, 0),
            zgui_geom::Size::new(self.mode.width() as i32, self.mode.height() as i32),
        );
        let Buffers::DrawnGl {
            stage,
            stale,
            composite,
            newest,
            ..
        } = &mut self.buffers
        else {
            return;
        };
        let Some(copier) = composite.as_mut() else {
            return;
        };
        let stage = *stage;

        let spelled = |held: &DamageSet<STALE>| {
            if held.is_full() {
                vec![whole]
            } else {
                held.rects().to_vec()
            }
        };
        // **The donor is the newest picture, so it lacks nothing.** What a buffer owes is measured
        // against the last one composited, and that one is complete by construction: it was brought
        // up to date and then had its own frame drawn on it. So the debt is copied whole, and there
        // is no second set to subtract — which is what makes this arithmetic a list rather than a
        // pass over every pair of rectangles.
        let donor = newest.filter(|newest| *newest != slot);
        *newest = Some(slot);
        let debt = spelled(&stale[slot]);
        stale[slot] = DamageSet::<STALE>::new();
        for (index, held) in stale.iter_mut().enumerate() {
            if index == slot {
                continue;
            }
            for rect in wrote {
                held.absorb(*rect);
            }
        }

        // **One issue, one wait.** The repair goes first because this frame's damage lands on top
        // of it, and both are given to the device together: asking for them one at a time would
        // wait for the first before the second could be recorded, and that wait is on the frame
        // loop's own thread.
        let mut steps: Vec<(usize, usize, Vec<zgui_scanout::Rect>)> = Vec::with_capacity(2);
        if let Some(donor) = donor
            && !debt.is_empty()
        {
            steps.push((donor, slot, named(&debt)));
        }
        steps.push((DRAWN + stage, slot, named(wrote)));
        let asked: Vec<(usize, usize, &[zgui_scanout::Rect])> = steps
            .iter()
            .map(|(from, to, rects)| (*from, *to, rects.as_slice()))
            .collect();
        if let Err(reason) = zgui_scanout::Copier::begin_all(copier, &asked) {
            warn!("this frame could not be built on a scanout buffer: {reason}");
        }
        zgui_profile::latency::note_with("s.built", || {
            format!(
                "repair={} apply={} owed={}",
                debt.len(),
                wrote.len(),
                stale[slot].len()
            )
        });
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
    /// `IN_FENCE_FD` property, a graphics driver that exports no sync file — the wait falls to this
    /// side, and this **defers** it. The frame is recorded as outstanding and [`Scanout::settle`]
    /// waits for it at the next acquire, so the loop's next front half runs while the card is still
    /// drawing this one. The frame reaches the screen one acquire later, which is what `true` says
    /// here and why an idle loop settles before it blocks.
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
        gpu: &Gpu,
        wrote: &[Rect<i32, DeviceSpace>],
    ) -> Result<bool, PlatformError> {
        if let Buffers::DrawnGl { signal, fenced, .. } = &self.buffers {
            let (signal, fenced) = (*signal, *fenced);
            let Some(slot) = self.rotation.drawn() else {
                return Ok(false);
            };
            // The renderer drew into a staging buffer and not into this one. Its commands have to
            // reach the kernel before the display's own device is asked to read what it wrote, and
            // then the picture is built here — see `Scanout::composite`. The fence taken below is
            // the scanout buffer's own and covers both devices' writes to it, which is what makes
            // one descriptor enough to hold the flip back.
            zgui_profile::latency::mark("s.composite");
            gl::flush(gpu);
            self.composite(slot, wrote);
            // What stands in for the Vulkan barrier. The two descriptor tiers answer one here, and
            // where the commit takes a fence the frame goes to the driver at once with it.
            //
            // Marked either side, because the two halves fail differently: a long wait here is the
            // graphics device still drawing, and a long commit is the display engine.
            let answers = matches!(signal, gl::Signal::Kernel | gl::Signal::Written);
            let fence = answers
                .then(|| {
                    zgui_profile::latency::mark("s.fence");
                    let fence = gl::finish(gpu, signal, self.drawn_descriptor(slot));
                    // Whether the descriptor was **already** signalled when it was taken, which is
                    // the one thing that says whether it describes this frame at all. A card cannot
                    // have drawn a frame submitted a moment ago, so `already` on every frame means
                    // the command stream had not reached the kernel and the buffer answered a
                    // fence for the frame before it. Asked only under a trace: it is a poll.
                    zgui_profile::latency::note_with("s.drawn", || {
                        match fence.as_ref().map(|fence| already_drawn(fence.as_fd())) {
                            Some(true) => "already".to_owned(),
                            Some(false) => "drawing".to_owned(),
                            None => "no descriptor".to_owned(),
                        }
                    });
                    fence
                })
                .flatten();
            if answers && fenced {
                let Some(ready) = self.rotation.finished(slot, fence) else {
                    return Ok(true);
                };
                self.show(device, commit, ready)?;
                return Ok(true);
            }
            // Everything else leaves the frame outstanding and the wait to [`Scanout::settle`]: the
            // frame's own work is over, and everything the loop does next is work the card can be
            // drawing underneath.
            //
            // A descriptor here is one the loop parks on beside the card and the input devices, so
            // it learns the frame is drawn where it is allowed to block. Nothing here — a tier that
            // answers none, or a buffer that would not say — leaves `watched` empty, and the wait
            // falls to this thread at the next acquire.
            let watched = fence.map(watched::Watched::on);
            if let Buffers::DrawnGl { submitted, .. } = &mut self.buffers {
                submitted.push_back(Outstanding {
                    slot,
                    since: Instant::now(),
                    watched,
                });
            }
            // After the frame is recorded as outstanding, so the buffer stays the caller's while
            // the card draws it and the next frame is composed into another one.
            self.rotation.submitted(slot);
            return Ok(true);
        }
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

    /// Waits for the frame [`Scanout::present_drawn`] left outstanding, and gives its buffer over.
    ///
    /// A machine whose driver exports no sync file has to wait for its own drawing, and that wait
    /// is the whole time the card takes — 23 ms of a 34 ms frame on the slowest machine this runs
    /// on. Paying it where the frame is submitted leaves the processor idle for every one of them.
    /// So the frame is recorded as outstanding instead and the wait happens here, and everything
    /// the loop does in between — the rest of the frame, and the next frame's events, style, layout
    /// and paint — runs while the card draws.
    ///
    /// **Called from [`Scanout::acquire`]**, which is the last moment it can be: the buffer the next
    /// frame is drawn into cannot be the one the outstanding frame is in, and the outstanding frame
    /// has to reach the driver for its buffer to move on. A loop about to block calls it too, so
    /// that the last frame of an animation is not left waiting for a frame that never comes.
    ///
    /// Does nothing for a display with nothing outstanding, which is every display on the copied
    /// and imported shapes and one that has not drawn since it last settled.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses the mode or the flip. The frame
    /// is no longer outstanding either way: a flip refused once is refused again, and a display
    /// that kept it would never take another frame.
    pub fn settle(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        gpu: &Gpu,
    ) -> Result<(), PlatformError> {
        loop {
            let Buffers::DrawnGl {
                signal, submitted, ..
            } = &self.buffers
            else {
                return Ok(());
            };
            let signal = *signal;
            let Some(outstanding) = submitted.front() else {
                return Ok(());
            };
            // A frame with a descriptor is given over when that descriptor says the card has
            // finished, and left alone until then. Nothing blocks here: the loop keeps its timers,
            // its input and its page flips, and the descriptor is in the wait it parks on.
            //
            // The oldest first and no further: the fences signal in the order the frames were
            // submitted, so a younger one cannot be finished before it.
            if outstanding
                .watched
                .as_ref()
                .is_some_and(|held| !held.drawn())
            {
                return Ok(());
            }
            // A frame with a finished one behind it is a picture nobody will ever see: the display
            // takes one frame per vertical blank, so showing it would spend a whole refresh interval
            // putting up an image the next flip replaces, and delay that newer image by the same.
            // So it is dropped here and its buffer goes back.
            //
            // Only where the one behind carries a descriptor. Where it does not, finding out costs
            // a wait for the card, and a frame is not worth waiting for in order to throw it away.
            if submitted
                .get(1)
                .and_then(|next| next.watched.as_ref())
                .is_some_and(watched::Watched::drawn)
            {
                let Buffers::DrawnGl { submitted, .. } = &mut self.buffers else {
                    return Ok(());
                };
                let Some(stale) = submitted.pop_front() else {
                    return Ok(());
                };
                self.rotation.refused(stale.slot);
                zgui_profile::latency::mark("s.skipped");
                continue;
            }
            let watched = outstanding.watched.is_some();
            let Buffers::DrawnGl { submitted, .. } = &mut self.buffers else {
                return Ok(());
            };
            let Some(outstanding) = submitted.pop_front() else {
                return Ok(());
            };
            let slot = outstanding.slot;
            // Marked either side, because the two halves fail differently: a long wait here is the
            // graphics device still drawing, and a long commit is the display engine. A frame whose
            // descriptor has signalled has nothing left to wait for and the two marks fall
            // together.
            zgui_profile::latency::mark("s.fence");
            let fence = if watched {
                None
            } else {
                gl::finish(gpu, signal, self.drawn_descriptor(slot))
            };
            zgui_profile::latency::mark("s.drawn");
            // The moment the graphics device is known to have finished something, which is the one
            // moment asking it costs nothing. `Gpu::reclaim` says why that matters; what comes back
            // here is the staging memory the frame just finished was written through, in time for
            // the frame after next to be written through it again instead of allocating.
            gpu.reclaim();
            // A frame already waiting for a completion is one this frame supersedes, and neither has
            // reached the driver. Taking its place costs nothing and saves the refresh interval the
            // older one would have spent on the screen before this one replaced it.
            if self.rotation.holding() {
                if let Some(stale) = self.rotation.replaces_held(slot, fence) {
                    self.rotation.refused(stale);
                }
                zgui_profile::latency::mark("s.skipped");
                continue;
            }
            let Some(ready) = self.rotation.finished(slot, fence) else {
                continue;
            };
            if let Err(refusal) = self.show(device, commit, ready) {
                // The display never took this buffer, so the next frame may have it. Without this
                // the rotation would lose one buffer to every refusal and eventually hand out none.
                self.rotation.refused(slot);
                return Err(refusal);
            }
        }
    }

    /// Settles a frame the loop is about to block too long to reach an acquire for.
    ///
    /// What a loop calls before it waits, where [`Scanout::settle`] is what an acquire calls.
    /// `within` is how long that wait is, and nothing where the loop is about to block until
    /// something happens.
    ///
    /// **Where the frame carries a descriptor this always settles**, because it never blocks:
    /// asking whether the card has finished is a poll of no length, and the loop is parked on that
    /// same descriptor, so the wake that brings it here is the frame itself.
    ///
    /// Where it carries none — a buffer that would not say what writes it — the wait falls to this
    /// thread, and then a frame younger than one refresh, before a wait shorter than one refresh, is left
    /// outstanding: the acquire the frame after it makes falls inside that window. One refresh,
    /// because that is how long a frame can usefully wait: one that has not reached an acquire by
    /// then has missed the flip it would have been shown at. Both halves are needed. The wait
    /// covers an application that stops animating and parks, and the age covers one that keeps
    /// waking for something else and never draws again.
    ///
    /// # Errors
    ///
    /// As [`Scanout::settle`].
    pub fn settle_before_blocking(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        gpu: &Gpu,
        within: Option<Duration>,
    ) -> Result<(), PlatformError> {
        let Buffers::DrawnGl { submitted, .. } = &self.buffers else {
            return Ok(());
        };
        let Some(outstanding) = submitted.front() else {
            return Ok(());
        };
        // A frame carrying a descriptor costs nothing to ask about and blocks nothing, so it is
        // always asked: that descriptor is what brought the loop here, and this is where the frame
        // reaches the screen.
        if outstanding.watched.is_some() {
            return self.settle(device, commit, gpu);
        }
        let refresh = self.refresh_interval();
        let soon = within.is_some_and(|wait| wait <= refresh);
        let fresh = outstanding.since.elapsed() <= refresh;
        if soon && fresh {
            return Ok(());
        }
        self.settle(device, commit, gpu)
    }

    /// Returns how long this display takes to show one frame.
    ///
    /// Worked out from the mode rather than stated, and a mode that reports no rate at all answers
    /// a sixtieth of a second — which is what nearly every display this runs on does anyway.
    fn refresh_interval(&self) -> Duration {
        match self.mode.refresh_rate_millihertz() {
            0 => Duration::from_nanos(16_666_667),
            rate => Duration::from_nanos(1_000_000_000_000 / u64::from(rate)),
        }
    }

    /// Reads this display's completion out of `events`, and shows the frame that waited for it.
    ///
    /// Answers when the vertical blank was, on the kernel's monotonic clock, for the completion
    /// it consumed. That moment is the display's own phase, which is what paces the frame after
    /// it; nothing while no completion of this display's arrived.
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
    ) -> Result<Option<Duration>, PlatformError> {
        let Some(at) = completed(events, self.pipe.crtc) else {
            return Ok(None);
        };
        self.owed_since = None;
        if let Some(ready) = self.rotation.completed() {
            self.show(device, commit, ready)?;
        }
        Ok(Some(at))
    }

    /// When the completion this display is waiting for stops being waited for.
    ///
    /// Nothing while no flip is outstanding. The loop parks no longer than this, because nothing
    /// else wakes it for a completion that is not coming.
    pub fn overdue(&self) -> Option<Instant> {
        self.owed_since.map(|since| self.watchdog().expiry(since))
    }

    /// Gives up on a completion that has not arrived, and carries on as though it had.
    ///
    /// Answers whether it gave up on one. A completion is the only thing that frees the buffer the
    /// display was reading, so one that never arrives leaves `Rotation::drawing` refusing a
    /// buffer to every frame for the rest of the program — a display that goes dark and stays dark.
    /// The same state a session coming back recovers from, reached without anyone taking the
    /// terminal away.
    ///
    /// The flip is treated as having landed, because on the evidence it did: the driver took it,
    /// and what went missing is the report. Treating it as refused instead would draw the next
    /// frame into the buffer the display is reading out of, which tears.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the driver refuses the held frame's flip.
    pub fn abandon(
        &mut self,
        device: &Device,
        commit: &mut dyn Commit,
        now: Instant,
    ) -> Result<bool, PlatformError> {
        let Some(since) = self.owed_since else {
            return Ok(false);
        };
        let watchdog = self.watchdog();
        if !watchdog.expired(since, now) {
            return Ok(false);
        }
        warn!(
            target: "zgui::platform",
            grace_ms = watchdog.grace().as_millis() as u64,
            "no flip completion arrived in time, so the frame behind it is taken as shown"
        );
        self.owed_since = None;
        let Some(ready) = self.rotation.completed() else {
            return Ok(true);
        };
        self.show(device, commit, ready)?;
        Ok(true)
    }

    /// How long this display's completions are waited for.
    fn watchdog(&self) -> Watchdog {
        Watchdog::for_interval(Some(refresh_interval(Some(
            self.mode.refresh_rate_millihertz(),
        ))))
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
            Buffers::DrawnGl {
                buffers, handles, ..
            } => {
                // The handles first: an allocation the kernel still holds a name for is one the
                // driver will not free, and the textures below are what release the allocations.
                for handle in handles {
                    if let Err(error) = device.release_imported(handle) {
                        warn!("an imported scanout buffer could not be released: {error}");
                    }
                }
                // wgpu deletes each texture as it reaches it, and a gbm allocation goes with the
                // buffer that holds it. A buffer the *kernel's* allocator made holds a GEM handle
                // instead, and destroying one needs the device — which a destructor has no way to
                // reach, so it is given back here.
                for buffer in buffers {
                    buffer.release(device);
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

    /// Allocates buffers for `output` that a GL renderer composes into and the display reads.
    ///
    /// The counterpart to [`Scanout::imported`] for a machine with no Vulkan, and the same
    /// arrangement reached from the other end: `libgbm` allocates on this display's own node and
    /// EGL imports each descriptor as a texture, rather than Vulkan creating an image and
    /// exporting it.
    ///
    /// **The display's node and not the renderer's.** A buffer allocated on the card that renders
    /// is moved out of that card's own memory the moment the display card imports the descriptor,
    /// and the submission that would draw into it is then refused. Where the two are one card the
    /// question does not arise and this allocates from the only device there is.
    ///
    /// # Errors
    ///
    /// Returns [`Copied`], which names why this display falls back to copying each frame.
    pub fn drawn_gl(
        device: &Device,
        output: &Output,
        gpu: &Gpu,
        pointer_on_a_plane: bool,
    ) -> Result<Self, Copied> {
        if !pointer_on_a_plane {
            return Err(Copied::NoCursorPlane);
        }
        let library = gbm::Library::load().map_err(|reason| {
            Copied::NoImages(Unsupported::Driver {
                step: "loading libgbm, which is what allocates a buffer both ends can use",
                reason,
            })
        })?;
        let allocator = gbm::Device::new(&library, device.as_fd()).map_err(|reason| {
            Copied::NoImages(Unsupported::Driver {
                step: "opening an allocator over the display's own node",
                reason,
            })
        })?;

        let width = output.mode.width();
        let height = output.mode.height();
        let buffers = gl::create_agreed(gpu, &allocator, device, width, height, DRAWN)
            .map_err(Copied::NoImages)?;
        // Allocated the same way and on the same node, and registered as no framebuffer: nothing
        // scans one out. `STAGING` says what they are for.
        let staging = gl::create_agreed(gpu, &allocator, device, width, height, STAGING)
            .map_err(Copied::NoImages)?;

        // Whether the display takes an in-fence at all decides the top tier: a descriptor the
        // driver would export has nowhere to go on a display that cannot be handed one.
        let fenced = waits_for_a_fence(device, output.pipe.plane).unwrap_or_else(|error| {
            warn!(
                plane = output.pipe.plane,
                "this plane's properties could not be read, so this display waits for its own                  frames rather than handing the kernel a fence: {error}"
            );
            false
        });
        let mut buffers = buffers;
        let mut handles = Vec::with_capacity(buffers.len());
        let mut framebuffers = Vec::with_capacity(buffers.len());
        for buffer in &mut buffers {
            let (stride, offset, modifier) = (buffer.stride(), buffer.offset(), buffer.modifier());
            let descriptor = buffer.descriptor().map_err(|reason| {
                Copied::NoImages(Unsupported::Driver {
                    step: "exporting a scanout buffer as a descriptor",
                    reason,
                })
            })?;
            let imported = device.import_buffer(descriptor).map_err(|error| {
                Copied::NoImages(Unsupported::Driver {
                    step: "importing a scanout buffer's descriptor",
                    reason: error.to_string(),
                })
            })?;
            // The layout is named only where the driver named one. A chain is wholly implicit or
            // wholly explicit, and `DRM_FORMAT_MOD_INVALID` is not a layout.
            let named = (device.supports_format_modifiers() && modifier != gbm::IMPLICIT)
                .then_some(Modifier(modifier));
            let framebuffer = device
                .add_framebuffer_from_handles(
                    width,
                    height,
                    fourcc(BGRA),
                    [imported.handle(), 0, 0, 0],
                    [stride, 0, 0, 0],
                    [offset, 0, 0, 0],
                    named,
                )
                .map_err(|error| {
                    Copied::NoImages(Unsupported::Driver {
                        step: "registering a framebuffer over a scanout buffer",
                        reason: error.to_string(),
                    })
                })?;
            handles.push(imported);
            framebuffers.push(framebuffer);
        }

        // Asked of a buffer rather than of the device, and therefore only now: the answer is
        // whether this kernel says what is writing one of them. A buffer nothing has drawn into yet
        // answers a descriptor that is already signalled, which is an answer and not a refusal.
        let from_buffers = buffers.first().and_then(gl::Drawn::exported).is_some_and(
            |descriptor| match zgui_drm::sync::writers_of(descriptor) {
                Ok(fence) => fence.is_some(),
                Err(refusal) => {
                    warn!(
                        "this kernel would not say what is writing a scanout buffer, so the frame \
                         loop waits for the graphics device instead: {refusal}"
                    );
                    false
                }
            },
        );
        let signal = gl::signal(gpu, fenced, from_buffers);
        // Which of the four, and the two facts that chose it. A frame's largest single cost on a
        // machine whose driver exports no sync file is who waits for the card, and reading that off
        // a trace means guessing at it from where the frame stalled.
        info!(
            crtc = output.pipe.crtc,
            plane = output.pipe.plane,
            atomic = device.is_atomic(),
            ?signal,
            plane_takes_a_fence = fenced,
            buffers_answer = from_buffers,
            "this display's frames are waited for"
        );

        // The device that composites. It holds the scanout buffers first and the staging buffers
        // after them, so a scanout slot is its own index and a staging one is `DRAWN + stage`.
        let mut staging = staging;
        let composite = composite_over(device, &buffers, &mut staging);
        if composite.is_none() {
            return Err(Copied::NoImages(Unsupported::Driver {
                step: "opening the display's own device to composite frames onto its buffers",
                reason: "the display's node would not give a copier".to_owned(),
            }));
        }

        Ok(Self::new(
            output,
            Buffers::DrawnGl {
                buffers,
                handles,
                framebuffers,
                signal,
                fenced,
                submitted: VecDeque::new(),
                staging,
                stage: 0,
                // Nothing has been shown, so every buffer owes the whole picture. The first frame
                // into each of them redraws it, which is what makes the debt true.
                stale: vec![DamageSet::<STALE>::full(); DRAWN],
                composite,
                newest: None,
            },
        ))
    }

    /// Creates a scanout for `output` holding `buffers`, with nothing on the screen yet.
    fn new(output: &Output, buffers: Buffers) -> Self {
        // Nothing is on the screen, so every buffer is free: the first frame goes into the first of
        // them and the modeset puts that one up.
        let count = match &buffers {
            Buffers::Copied { buffers, .. } => buffers.len(),
            Buffers::DrawnGl { buffers, .. } => buffers.len(),
            Buffers::Imported { buffers, .. } => buffers.len(),
        };
        let rotation = Rotation::new(count);
        Self {
            pipe: output.pipe,
            mode: output.mode,
            buffers,
            rotation,
            lit: false,
            owed_since: None,
            // Every buffer holds nothing, so every one of them owes the whole frame. The bound is
            // the display's own height, which the first copy cuts to whatever it is given.
            stale: vec![zgui_render_wgpu::frame::damage::every_row(output.mode.height()); count],
        }
    }

    /// Returns the framebuffer the kernel knows the buffer at `slot` by.
    ///
    /// `slot` is one the rotation answered, so it names a buffer of this display.
    fn framebuffer(&self, slot: usize) -> Framebuffer {
        match &self.buffers {
            Buffers::Copied { framebuffers, .. } => framebuffers[slot],
            Buffers::DrawnGl { framebuffers, .. } | Buffers::Imported { framebuffers, .. } => {
                framebuffers[slot]
            }
        }
    }

    /// Returns the descriptor the buffer at `slot` was exported as, on the drawn shape.
    ///
    /// What [`gl::Signal::Written`] asks for the frame's fence. Nothing where this display draws no
    /// buffers of its own, and nothing where the buffer was never exported — neither is reachable
    /// under that tier, and both answer the same way a kernel that will not say does.
    fn drawn_descriptor(&self, slot: usize) -> Option<BorrowedFd<'_>> {
        match &self.buffers {
            Buffers::DrawnGl { buffers, .. } => buffers.get(slot)?.exported(),
            Buffers::Copied { .. } | Buffers::Imported { .. } => None,
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
            // Read here rather than passed in: this is the moment the driver took the flip, which
            // is what the wait for its completion is measured from.
            self.owed_since = Some(Instant::now());
        } else {
            self.lit = true;
            self.rotation.shown(slot);
            // A modeset carries no completion, so nothing is owed after one.
            self.owed_since = None;
        }
        // Marked here rather than at the callers, because a frame reaches the driver from three
        // places — the settle that finds it finished, the completion that takes a held one, and the
        // restore that puts a display back — and only one of them used to say so. A trace that
        // counts these counts every picture that reached a screen.
        zgui_profile::latency::mark("s.flipped");
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

/// Copies the rows `rows` names from `source` into `destination`, each stepping by its own stride.
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
    rows: Range<u32>,
) {
    if source_stride == 0 || destination_stride == 0 {
        return;
    }
    // A destination narrower than the source is the same truncation one row along.
    let width = source_stride.min(destination_stride);
    let start = rows.start as usize;
    let count = rows.end.saturating_sub(rows.start) as usize;
    for (into, from) in destination
        .chunks_exact_mut(destination_stride)
        .skip(start)
        .zip(source.chunks_exact(source_stride).skip(start))
        .take(count)
    {
        into[..width].copy_from_slice(&from[..width]);
    }
}

/// Returns when a flip on `crtc` finished, if any of `events` says one did.
///
/// The moment is the vertical blank's, on the kernel's monotonic clock. It is what a display knows
/// about its own timing that nothing above it can work out: the nominal refresh rate gives the
/// interval between blanks and says nothing about where in one the display currently is.
fn completed(events: &[Event], crtc: u32) -> Option<Duration> {
    events.iter().find_map(|event| match event {
        Event::FlipComplete {
            crtc: finished, at, ..
        } if *finished == crtc => Some(*at),
        _ => None,
    })
}

/// Records against every buffer the rows a frame changed.
///
/// What a buffer is then owed is taken out of its own entry, which leaves it owing nothing: the
/// copy that follows writes exactly those rows into it. Free of the rotation and of the device,
/// because neither is what makes it right.
fn owe(stale: &mut [Vec<Range<u32>>], changed: &[Range<u32>], height: u32) {
    for held in stale {
        *held = merge(held, changed, height);
    }
}

/// The bands of `held` and `adding` together, merged, in order and cut to `height`.
///
/// Answers the whole of it rather than a long list where the two make more bands than are worth
/// tracking: many scattered copies cost more in calls than one copy of everything saves in bytes.
fn merge(held: &[Range<u32>], adding: &[Range<u32>], height: u32) -> Vec<Range<u32>> {
    let mut bands: Vec<Range<u32>> = held
        .iter()
        .chain(adding)
        .map(|band| band.start.min(height)..band.end.min(height))
        .filter(|band| band.start < band.end)
        .collect();
    bands.sort_unstable_by_key(|band| band.start);
    let mut kept: Vec<Range<u32>> = Vec::with_capacity(bands.len());
    for band in bands {
        match kept.last_mut() {
            // Touching counts as overlapping: two bands that meet exactly are one copy.
            Some(last) if band.start <= last.end => last.end = last.end.max(band.end),
            _ => kept.push(band),
        }
    }
    if kept.len() > MOST_BANDS {
        return zgui_render_wgpu::frame::damage::every_row(height);
    }
    kept
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

/// Whether `fence` has already signalled, which is a poll of no length.
///
/// For the trace above and for nothing else. Answers `true` where the kernel refuses the question,
/// which is the same answer [`watched::Watched::drawn`] gives and for the same reason.
fn already_drawn(fence: BorrowedFd<'_>) -> bool {
    let mut asked = [rustix::event::PollFd::from_borrowed_fd(
        fence,
        rustix::event::PollFlags::IN,
    )];
    !matches!(
        rustix::event::poll(&mut asked, Some(&rustix::event::Timespec::default())),
        Ok(0)
    )
}

/// Opens the device that composites a frame onto a scanout buffer, over the display's own node.
///
/// It is given the scanout buffers first and the staging buffers after them, so a scanout slot is
/// its own index and a staging one is `DRAWN + stage`. One copier holding both is what lets a
/// composite be two copies into the same buffer rather than two copiers arguing over a thread.
///
/// Answers nothing where the node gives no copier — no EGL, no import of a descriptor, no way to
/// copy. The caller falls back to a shape that needs none.
fn composite_over(
    device: &Device,
    scanout: &[gl::Drawn],
    staging: &mut [gl::Drawn],
) -> Option<zgui_scanout::egl::Egl> {
    // Exported in one pass and read back in another: the export takes `&mut`, and a list of
    // descriptors is several of those borrows at once.
    for buffer in staging.iter_mut() {
        if let Err(reason) = buffer.descriptor() {
            warn!("a staging buffer would not export, so nothing can composite it: {reason}");
            return None;
        }
    }
    let mut described = Vec::with_capacity(scanout.len() + staging.len());
    for buffer in scanout.iter().chain(staging.iter()) {
        let modifier = buffer.modifier();
        described.push(zgui_scanout::Buffer {
            descriptor: buffer.exported()?,
            width: buffer.texture().width(),
            height: buffer.texture().height(),
            fourcc: gl::FOURCC,
            stride: buffer.stride(),
            offset: buffer.offset(),
            // Named only where the driver named one, as the allocation asked for it.
            modifier: (modifier != gbm::IMPLICIT).then_some(modifier),
        });
    }
    match zgui_scanout::egl::Egl::open(device.as_fd(), &described) {
        Ok(copier) => {
            info!(
                target: "zgui::platform",
                "this display composites its own frames: {copier:?}"
            );
            Some(copier)
        }
        Err(reason) => {
            info!(
                target: "zgui::platform",
                "this display's own device cannot composite ({reason}), so it is driven another way"
            );
            None
        }
    }
}

/// The rectangles a copier is asked for, in its own shape.
fn named(rects: &[Rect<i32, DeviceSpace>]) -> Vec<zgui_scanout::Rect> {
    rects
        .iter()
        .map(|rect| zgui_scanout::Rect {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        })
        .collect()
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

    use super::{Layout, MOST_BANDS, PLANES, blit, completed, fourcc, layout, merge, owe};
    use crate::import::Plane;
    use std::ops::Range;
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

        blit(&source, 4, &mut destination, 6, 0..2);

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

        blit(&source, 3, &mut destination, 3, 0..2);

        assert_eq!(destination, source, "nothing is left over on either side");
    }

    #[test]
    fn no_rows_copies_nothing() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 4, &mut destination, 4, 0..0);

        assert_eq!(destination, [0xAA; 4], "a frame of no rows writes no bytes");
    }

    #[test]
    fn a_destination_too_short_takes_the_rows_that_fit_and_no_more() {
        // Three rows asked for, two rows of room. The alternative to truncating is an index past
        // the end, which in a frame loop is a panic per frame.
        let source = [1, 2, 3, 4, 5, 6];
        let mut destination = padded(4, 2);

        blit(&source, 2, &mut destination, 4, 0..3);

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

        blit(&source, 2, &mut destination, 4, 0..3);

        assert_eq!(
            destination,
            [
                1, 2, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA
            ],
            "one whole row is there, and the half row after it is not written"
        );
    }

    #[test]
    fn a_band_starting_part_way_down_copies_those_rows_and_leaves_the_others() {
        let source = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut destination = padded(2, 4);

        blit(&source, 2, &mut destination, 2, 1..3);

        assert_eq!(
            destination,
            [0xAA, 0xAA, 3, 4, 5, 6, 0xAA, 0xAA],
            "both sides skip to the band, so a row lands where it came from"
        );
    }

    /// The whole of what the copied shape depends on: a row changed while one buffer is being
    /// written is owed to every other, and a buffer that has just been written owes nothing.
    #[expect(
        clippy::single_range_in_vec_init,
        reason = "one band of rows a frame changed, which is what a frame usually changes"
    )]
    #[test]
    fn a_row_one_frame_changed_is_owed_to_the_buffer_the_next_frame_uses() {
        let mut stale = vec![Vec::new(), Vec::new()];

        let first: Vec<Range<u32>> = vec![0..10];
        owe(&mut stale, &first, 100);
        assert_eq!(core::mem::take(&mut stale[0]), vec![0..10]);

        let second: Vec<Range<u32>> = vec![20..30];
        owe(&mut stale, &second, 100);
        assert_eq!(
            core::mem::take(&mut stale[1]),
            vec![0..10, 20..30],
            "the buffer the first frame did not land in is owed that frame's rows as well"
        );

        owe(&mut stale, &[], 100);
        assert_eq!(
            stale[0],
            vec![20..30],
            "and the first buffer still owes the second frame's"
        );
    }

    #[test]
    fn bands_that_meet_or_overlap_become_one() {
        assert_eq!(
            merge(&[0..4, 10..12], &[4..6, 11..20], 100),
            vec![0..6, 10..20]
        );
    }

    #[expect(
        clippy::single_range_in_vec_init,
        reason = "one band, reaching past the bottom, which is what this is about"
    )]
    #[test]
    fn a_band_reaching_past_the_display_is_cut_to_it() {
        assert_eq!(merge(&[90..200], &[], 100), vec![90..100]);
        assert!(merge(&[200..300], &[], 100).is_empty());
    }

    /// Many scattered bands cost more in calls than one copy of everything saves in bytes.
    #[test]
    fn more_bands_than_are_worth_tracking_become_the_whole_frame() {
        let scattered: Vec<Range<u32>> = (0..MOST_BANDS as u32 + 1)
            .map(|n| n * 4..n * 4 + 1)
            .collect();
        assert_eq!(merge(&scattered, &[], 100), vec![0..100]);
    }

    #[test]
    fn a_stride_of_zero_copies_nothing_rather_than_dividing_by_it() {
        let source = [1, 2, 3, 4];
        let mut destination = padded(4, 1);

        blit(&source, 0, &mut destination, 4, 0..1);
        blit(&source, 4, &mut destination, 0, 0..1);

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
        assert_eq!(completed(&[flip(62)], 62), Some(Duration::from_secs(1)));
        assert_eq!(
            completed(&[flip(81), flip(62)], 62),
            Some(Duration::from_secs(1)),
            "one read carries the completions of every display that finished"
        );
    }

    #[test]
    fn a_completion_naming_another_crtc_belongs_to_another_display() {
        assert_eq!(completed(&[flip(81)], 62), None);
        assert_eq!(
            completed(&[], 62),
            None,
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
