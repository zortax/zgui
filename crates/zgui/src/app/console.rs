//! What a display is drawn through on a console.
//!
//! A window system presents for a caller; a console does not, so the last step belongs to whatever
//! draws. [`Renderer::draw`] is the frame boundary and the only one: nothing above it is told when
//! a frame begins or ends, and nothing below it is asked to present. [`DrmRenderer`] is that step.
//! It owns a `WgpuRenderer` and the display it draws to, forwards every method, and in
//! [`Renderer::draw`] draws the frame and puts it on the screen.
//!
//! It lives here rather than in the backend because a renderer is built by a [`RendererFactory`],
//! which the runtime owns, and the console backend sits far below the runtime. The backend offers
//! the four things this needs: the texture format a display can scan out, the map from a surface to
//! the display behind it, the buffers a display hands out, and the flip.
//!
//! # What a frame costs
//!
//! [`Delivery`] is the decision, taken once when the renderer is built and never per frame.
//!
//! **Drawn.** Nothing. The display hands out the buffers it scans out of, the renderer composes
//! straight into whichever one is free, and the frame reaches the screen where it already lies.
//!
//! **Copied.** One readback of the whole target and one copy of it into a buffer the display scans
//! out of, both on the thread that also reads input. This is the path for a display that cannot
//! hand out its own buffers, and every machine can run it.
//!
//! # The order of a drawn frame
//!
//! Take the buffer back, compose into that buffer, give it over. [`drawn_into_scanout`] holds that
//! order, and the order is the one thing on this path that fails silently: a frame composed before
//! the acquire is drawn into memory the display engine still owns, which is undefined and which
//! every ioctl below it reports as success.
//!
//! A frame that drew nothing gives nothing over. The buffer stays in the renderer's own queue
//! family, where the next frame needs it, so the acquire after a skipped frame names the same
//! buffer and costs nothing. Giving it over instead would put whatever is in it on the screen,
//! which is the picture from three frames ago.
//!
//! # The two methods that are not a plain forward
//!
//! [`Renderer::as_any_mut`] answers with the *inner* renderer. `zgui-wgpu` reaches the concrete
//! backend through it to fill `surface` elements, and a decorator answering with itself would leave
//! every one of those elements empty while nothing reported a fault.
//!
//! [`Renderer::draw`] is where the presentation happens, and it reports what happened in the
//! contract's own vocabulary. A frame the display declined because a flip is still on its way is
//! [`SkipReason::Timeout`]: asking again after one refresh interval is when the buffer is free. On
//! the drawn path that answer comes *before* the frame is composed, because the acquire asks the
//! question, and the work is never done rather than done and dropped. A readback, a handover or a
//! flip that *failed* is [`SkipReason::Validation`], the nearest true statement the contract has:
//! the frame was submitted and something below the renderer refused to present it. No variant names
//! a kernel that refused a page flip.

use std::sync::Arc;

use tracing::warn;
use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{PlatformError, Surface};
use zgui_platform_drm::{Displays, DrmDisplay};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    ScrollShift, SkipReason, TargetPoolReport, TextureHandle, VectorStatus,
};
use zgui_render_wgpu::{Gpu, SharedGraphics, WgpuRenderer};
use zgui_runtime::{AppError, RendererFactory};
use zgui_scene::Scene;

/// How a composed frame reaches the display.
///
/// Settled when the renderer is built, from what the display hands out, and read once per frame.
/// The two paths share every method except [`Renderer::draw`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    /// The frame is composed into the display's own buffer, and the buffer is given over.
    Drawn,
    /// The frame is read back out of the renderer and copied into the display's buffer.
    Copied,
}

/// A renderer that draws a frame and puts it on a display.
///
/// Everything about drawing belongs to the renderer this wraps. What this adds is the last step a
/// console needs and a window system does not.
struct DrmRenderer {
    /// What draws.
    inner: WgpuRenderer,
    /// Where the frame it drew goes.
    display: DrmDisplay,
    /// How it gets there.
    delivery: Delivery,
}

impl DrmRenderer {
    /// Creates a renderer that draws with `inner` and presents to `display` the way `delivery`
    /// says.
    ///
    /// The two have to agree. [`Delivery::Drawn`] needs an `inner` presenting into the textures
    /// `display` handed out, and [`Delivery::Copied`] needs one composing into a texture of its
    /// own — a window surface answers no readback at all. [`renderer`] pairs them, from the one
    /// question [`DrmDisplay::textures`] answers.
    fn new(inner: WgpuRenderer, display: DrmDisplay, delivery: Delivery) -> Self {
        Self {
            inner,
            display,
            delivery,
        }
    }

    /// Draws the frame into the buffer the display is about to scan out of.
    ///
    /// The drawn path. [`drawn_into_scanout`] holds the order; this composes, once it has been told
    /// which buffer to compose into.
    fn drawn(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let inner = &mut self.inner;
        drawn_into_scanout(&self.display, |slot| {
            if !inner.present_into(slot) {
                warn!(
                    "this renderer was not built over the buffers this display hands out, so slot \
                     {slot} named nothing and the frame would land wherever the last one did"
                );
                return FrameOutcome::Skipped(SkipReason::Validation);
            }
            inner.draw(scene, damage)
        })
    }

    /// Draws the frame, reads it back and copies it into the display's buffer.
    ///
    /// The copied path.
    ///
    /// An undamaged frame goes on the screen anyway where the display's own frames carry the
    /// pointer: the picture is unchanged and the pointer drawn over it is not, so putting the same
    /// texture on the display again is the only thing that moves the cursor. A display with a
    /// cursor plane skips it, because the display engine has already moved the pointer and the
    /// picture underneath is the one on the screen.
    ///
    /// Every other outcome drew nothing at all — an unconfigured surface, a device that could not
    /// be rebuilt — and there is nothing to put anywhere.
    fn copied(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let drawn = self.inner.draw(scene, damage);
        let unchanged_but_owed = matches!(drawn, FrameOutcome::Skipped(SkipReason::Undamaged))
            && self.display.carries_the_pointer();
        if !matches!(drawn, FrameOutcome::Presented(_)) && !unchanged_but_owed {
            return drawn;
        }

        let Some(pixels) = self.inner.read_presented() else {
            warn!(
                "this renderer composes into a window surface rather than into a texture, so \
                 there is nothing to read back and nothing reaches the display"
            );
            return FrameOutcome::Skipped(SkipReason::Validation);
        };
        match self.display.present(&pixels) {
            Ok(true) => drawn,
            // The buffer this frame would be written into is the one still on the screen. The
            // frame's work is submitted and the target holds it, so the damage retires and another
            // frame is asked for — which the contract paces at one refresh interval, by which time
            // the flip has completed.
            Ok(false) => FrameOutcome::Skipped(SkipReason::Timeout),
            Err(error) => {
                warn!("a frame could not be put on its display: {error}");
                FrameOutcome::Skipped(SkipReason::Validation)
            }
        }
    }
}

/// The two calls that bracket a frame drawn straight into a display's own buffer.
///
/// [`DrmDisplay`] is the one implementation. The trait lets the order below be asserted with no
/// graphics device and no display open, and that order is the one property on this path that fails
/// silently when it is wrong.
trait Bracket {
    /// Takes the buffer the next frame goes into back from the display engine, and names it.
    fn acquire(&self) -> Result<Option<usize>, PlatformError>;

    /// Gives the buffer the frame was drawn into to the display engine, and flips to it.
    fn present_drawn(&self) -> Result<bool, PlatformError>;
}

impl Bracket for DrmDisplay {
    fn acquire(&self) -> Result<Option<usize>, PlatformError> {
        DrmDisplay::acquire(self)
    }

    fn present_drawn(&self) -> Result<bool, PlatformError> {
        DrmDisplay::present_drawn(self)
    }
}

/// Runs one frame on the drawn path: take the buffer back, compose into it, give it over.
///
/// `compose` is handed the buffer the frame goes into and answers what the frame did. It runs
/// **between** the two calls and never outside them: the acquire makes the buffer safe to write,
/// and the give-over makes the pixels it drew the pixels the display engine reads.
///
/// # A frame that drew nothing
///
/// A frame with no free buffer takes nothing back, so it owes nothing back.
///
/// A buffer that *was* taken back and drawn into nothing is left where it is. Nothing is given
/// over and nothing is committed, so the display keeps showing the frame it already has, and the
/// acquire before the next frame names the same buffer and finds it on this side of the handover
/// already. That covers a slot the renderer would not take, a set that no longer matches the
/// target, a device that could not be rebuilt, and a frame that damaged nothing.
fn drawn_into_scanout(
    display: &dyn Bracket,
    compose: impl FnOnce(usize) -> FrameOutcome,
) -> FrameOutcome {
    let slot = match display.acquire() {
        Ok(Some(slot)) => slot,
        // Every buffer is either on the screen or named by a flip still on its way. Nothing was
        // taken back, so nothing is owed back, and the frame is held off before any work is done
        // rather than composed and then dropped. The contract asks again one refresh later, by
        // which time the flip has completed.
        Ok(None) => return FrameOutcome::Skipped(SkipReason::Timeout),
        Err(error) => {
            warn!("the buffer a frame would be drawn into could not be taken back: {error}");
            return FrameOutcome::Skipped(SkipReason::Validation);
        }
    };

    let drawn = compose(slot);
    if !matches!(drawn, FrameOutcome::Presented(_)) {
        return drawn;
    }

    match display.present_drawn() {
        Ok(true) => drawn,
        // The acquire above answered, so the buffer was free when the frame started. A flip that
        // arrived in between is the display declining it, and the contract paces the next attempt
        // at one refresh interval.
        Ok(false) => FrameOutcome::Skipped(SkipReason::Timeout),
        Err(error) => {
            warn!("a frame could not be given to its display: {error}");
            FrameOutcome::Skipped(SkipReason::Validation)
        }
    }
}

impl Renderer for DrmRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    /// Draws the frame and puts it on the display, the way this renderer's [`Delivery`] says.
    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        match self.delivery {
            Delivery::Drawn => self.drawn(scene, damage),
            Delivery::Copied => self.copied(scene, damage),
        }
    }

    fn shifts_composed_pixels(&self) -> bool {
        self.inner.shifts_composed_pixels()
    }

    fn shift_composed(&mut self, shift: ScrollShift) {
        self.inner.shift_composed(shift);
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn vector_status(&self) -> VectorStatus {
        self.inner.vector_status()
    }

    fn target_pool(&self) -> TargetPoolReport {
        self.inner.target_pool()
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.inner.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.inner.release_idle_resources()
    }

    fn acquire_block(&self) -> core::time::Duration {
        self.inner.acquire_block()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }

    /// The renderer underneath, which is the one a backend-specific companion is looking for.
    ///
    /// Answering with this decorator instead would compile, pass every test that does not draw a
    /// `surface` element, and leave every one that does as empty space.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }
}

/// Makes the renderer factory an application's displays are drawn through on a console.
///
/// One graphics device behind all of them: `graphics` is opened before the first display exists,
/// because the images a display scans out of are created on it. The factory is called once per
/// display and keeps that device, its pipelines and its compiled-shader cache between calls.
///
/// `displays` is where a display is found: the frame loop writes each one in under the surface it
/// is seen as, and it is the same map [`zgui_platform_drm::run`] was given. The graphics is shared
/// with that loop for the same reason. All three are one decision, and
/// [`App::run_drm`](crate::App::run_drm) takes it.
///
/// # Errors
///
/// The factory returns [`AppError::GpuUnavailable`] when no adapter on this machine could compose a
/// frame, naming every one that was tried, and [`AppError::Platform`] when the surface it is handed
/// belongs to no running frame loop, which is the answer a factory installed on its own gets.
pub(crate) fn factory(graphics: SharedGraphics, displays: Displays) -> RendererFactory {
    Box::new(move |surface, target| renderer(&graphics, &displays, surface, target))
}

/// Opens the device a display's own buffers are made on.
///
/// Answers `None` on a machine with no usable adapter and on one whose driver would not grant the
/// Vulkan device extensions an exported image needs. Both are ordinary facts about a machine rather
/// than a failure to start: the answer to each is the copied path, which every machine has.
///
/// The extensions are read off the device that opened rather than assumed from what was asked for.
/// A device extension can be enabled only while a device is created, so a device without them is
/// the device this program has for the rest of its run.
pub(crate) fn scanout_device(graphics: &SharedGraphics) -> Option<Arc<Gpu>> {
    let gpu = match graphics.open_gpu() {
        Ok(gpu) => gpu,
        Err(failure) => {
            warn!(
                "no graphics device on this machine could compose a frame, so every display copies \
                 its frames through the processor: {failure}"
            );
            return None;
        }
    };
    let enabled = gpu.vulkan_extensions();
    if let Some(missing) = zgui_platform_drm::EXTENSIONS
        .iter()
        .find(|name| !enabled.contains(name))
    {
        warn!(
            "{} did not enable {missing:?}, so every display copies its frames through the \
             processor",
            gpu.describe()
        );
        return None;
    }
    Some(gpu)
}

/// Opens a renderer for `surface` on the shared graphics.
fn renderer(
    graphics: &SharedGraphics,
    displays: &Displays,
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let id = surface.id();
    let Some(display) = displays.for_surface(id) else {
        return Err(AppError::Platform(PlatformError::Backend(format!(
            "surface {id:?} is not a display any frame loop is driving, so a frame drawn for it \
             would reach no screen"
        ))));
    };

    // The one question that decides the path, asked once. A display that hands out buffers is one
    // whose frames are composed into it; an empty answer is a display that reads its frames back.
    let textures = display.textures();
    let (mut inner, delivery) = if textures.is_empty() {
        // The format is the backend's, because the buffers a display scans out of were allocated
        // with the fourcc its channel order picks. Mutable texture formats are not asked for:
        // nothing here views the target under a second format, and asking would cost the sRGB fast
        // path on adapters that offer one.
        let inner = graphics.renderer_offscreen(target, zgui_platform_drm::FORMAT, false)?;
        (inner, Delivery::Copied)
    } else {
        let inner = graphics.renderer_supplied(target, textures)?;
        (inner, Delivery::Drawn)
    };
    // Without this a display list's vector passes are planned, counted and then drawn from nothing,
    // so every drawing on the display — every icon — is empty space. Which rasteriser this is comes
    // from what the device turned out to be able to run, and the fallback needs no compute shaders.
    zgui_render_vector_vello::attach(&mut inner, target.size);
    Ok(Box::new(DrmRenderer::new(inner, display, delivery)))
}

#[cfg(test)]
mod tests {
    //! The order a drawn frame keeps, and what a frame that drew nothing leaves behind.
    //!
    //! Both are asserted through [`Bracket`] rather than against a display, because both are
    //! decisions this module takes and neither needs a device to be wrong. `tests/imported.rs` in
    //! `zgui-platform-drm` is where the same two run against a real driver and a real kernel.

    use super::*;

    use std::cell::{Cell, RefCell};

    use zgui_geom::{Device, DevicePx, Scale, Size};
    use zgui_platform::SurfaceId;
    use zgui_platform_headless::OffscreenSurface;
    use zgui_render::FrameStats;

    /// How many buffers the imported shape drives a display from.
    const BUFFERS: usize = 3;

    /// A frame that reached the buffer it was pointed at.
    fn presented() -> FrameOutcome {
        FrameOutcome::Presented(FrameStats::default())
    }

    /// A display that answers the way an imported scanout does, writing down what it was asked.
    ///
    /// It holds the two pieces of state the real pair keeps: which buffer the next frame goes into,
    /// and which buffers the display engine holds. `Handover::release` refuses a buffer the display
    /// engine already has, and `Scanout::present_drawn` is the only thing that moves to the next
    /// buffer — so a frame that gives nothing over leaves both of them where they were.
    struct Rotating {
        /// What was asked of it, in order, with the compose steps written in between.
        log: RefCell<Vec<String>>,
        /// Which buffer the next frame goes into.
        back: Cell<usize>,
        /// Whether the display engine holds each buffer.
        foreign: RefCell<[bool; BUFFERS]>,
        /// Whether a flip is still on its way, which leaves no buffer free.
        busy: Cell<bool>,
    }

    impl Rotating {
        /// A display with nothing on the screen and every buffer on this side of the handover.
        fn new() -> Self {
            Self {
                log: RefCell::new(Vec::new()),
                back: Cell::new(0),
                foreign: RefCell::new([false; BUFFERS]),
                busy: Cell::new(false),
            }
        }

        /// A display with every buffer either on the screen or named by a flip on its way.
        fn flipping() -> Self {
            let display = Self::new();
            display.busy.set(true);
            display
        }

        /// Writes `step` down.
        fn note(&self, step: impl Into<String>) {
            self.log.borrow_mut().push(step.into());
        }

        /// What was asked of it, in order.
        fn steps(&self) -> Vec<String> {
            self.log.borrow().clone()
        }

        /// One frame drawn through this display, composed into whatever it is pointed at.
        fn frame(&self, composed: FrameOutcome) -> FrameOutcome {
            drawn_into_scanout(self, |slot| {
                self.note(format!("compose into {slot}"));
                composed
            })
        }
    }

    impl Bracket for Rotating {
        fn acquire(&self) -> Result<Option<usize>, PlatformError> {
            if self.busy.get() {
                self.note("acquire: no buffer is free");
                return Ok(None);
            }
            let back = self.back.get();
            self.foreign.borrow_mut()[back] = false;
            self.note(format!("acquire {back}"));
            Ok(Some(back))
        }

        fn present_drawn(&self) -> Result<bool, PlatformError> {
            if self.busy.get() {
                self.note("give over: no buffer is free");
                return Ok(false);
            }
            let back = self.back.get();
            if self.foreign.borrow()[back] {
                return Err(PlatformError::Backend(format!(
                    "the display engine already holds buffer {back}, so it has to be acquired \
                     before a frame is drawn into it"
                )));
            }
            self.foreign.borrow_mut()[back] = true;
            self.back.set((back + 1) % BUFFERS);
            self.note(format!("give over {back}"));
            Ok(true)
        }
    }

    #[test]
    fn a_frame_is_composed_between_the_two_calls_that_bracket_it() {
        // The order in full. The acquire makes the buffer safe to write and says which buffer that
        // is, so a frame composed in front of it is drawn into memory the display engine still
        // owns — which is undefined and which every ioctl below reports as success.
        let display = Rotating::new();

        assert_eq!(display.frame(presented()), presented());

        assert_eq!(
            display.steps(),
            ["acquire 0", "compose into 0", "give over 0"],
            "the buffer is taken back, drawn into, and only then given over"
        );
    }

    #[test]
    fn each_frame_goes_into_the_buffer_the_display_named_for_it() {
        let display = Rotating::new();

        for _ in 0..BUFFERS + 1 {
            assert_eq!(display.frame(presented()), presented());
        }

        assert_eq!(
            display.steps(),
            [
                "acquire 0",
                "compose into 0",
                "give over 0",
                "acquire 1",
                "compose into 1",
                "give over 1",
                "acquire 2",
                "compose into 2",
                "give over 2",
                "acquire 0",
                "compose into 0",
                "give over 0",
            ],
            "the frame follows the display round its buffers rather than choosing one itself"
        );
    }

    #[test]
    fn a_frame_with_no_free_buffer_is_never_composed() {
        // The work is skipped before it is done rather than done and dropped. `Timeout` says so:
        // the buffer is busy, and asking again one refresh later is when it is free.
        let display = Rotating::flipping();

        assert_eq!(
            display.frame(presented()),
            FrameOutcome::Skipped(SkipReason::Timeout)
        );

        assert_eq!(
            display.steps(),
            ["acquire: no buffer is free"],
            "nothing was composed and nothing was given over"
        );
    }

    #[test]
    fn a_frame_that_composed_nothing_is_not_given_over() {
        // A buffer holds whatever was last drawn into it, which is the picture from three frames
        // ago. Giving it over would put that on the screen and report the frame as presented.
        let display = Rotating::new();

        let outcome = display.frame(FrameOutcome::Skipped(SkipReason::Unconfigured));

        assert_eq!(
            outcome,
            FrameOutcome::Skipped(SkipReason::Unconfigured),
            "what the renderer said about its own frame is what the caller is told"
        );
        assert_eq!(
            display.steps(),
            ["acquire 0", "compose into 0"],
            "the frame was never given to the display engine"
        );
    }

    #[test]
    fn an_undamaged_frame_is_not_put_on_the_screen_a_second_time() {
        // The copied path puts an unchanged picture up again where the display's own frames carry
        // the pointer. This path never does, and the reason is structural: a display here has a
        // cursor plane, because that is the condition `Scanout::imported` chose it under, so the
        // display engine has already moved the pointer and the picture under it is the one on the
        // screen. Nothing here reads `DrmDisplay::carries_the_pointer` at all.
        let display = Rotating::new();

        let outcome = display.frame(FrameOutcome::Skipped(SkipReason::Undamaged));

        assert_eq!(outcome, FrameOutcome::Skipped(SkipReason::Undamaged));
        assert_eq!(
            display.steps(),
            ["acquire 0", "compose into 0"],
            "a picture that did not change was flipped to a second time"
        );
    }

    #[test]
    fn a_frame_that_composed_nothing_leaves_the_next_one_the_buffer_it_took_back() {
        // The balance the handover needs. A buffer taken back and never given over stays in the
        // renderer's own queue family, which is where the next frame needs it: the acquire names
        // the same buffer, finds it on this side already, and the give-over after it is accepted.
        let display = Rotating::new();

        display.frame(FrameOutcome::Recovered);
        assert_eq!(display.frame(presented()), presented());

        assert_eq!(
            display.steps(),
            [
                "acquire 0",
                "compose into 0",
                "acquire 0",
                "compose into 0",
                "give over 0",
            ],
            "a skipped frame moved the display nowhere and left nothing owed"
        );
    }

    #[test]
    fn a_buffer_that_could_not_be_taken_back_composes_nothing() {
        struct Refusing;

        impl Bracket for Refusing {
            fn acquire(&self) -> Result<Option<usize>, PlatformError> {
                Err(PlatformError::Backend(
                    "the device did not finish the barrier".to_owned(),
                ))
            }

            fn present_drawn(&self) -> Result<bool, PlatformError> {
                panic!("a frame nothing was drawn for was given to the display engine");
            }
        }

        // `Validation` rather than `Timeout`: the buffer is not busy, the graphics device refused.
        // Asking again one refresh later would refuse again.
        assert_eq!(
            drawn_into_scanout(&Refusing, |_| panic!(
                "a buffer the display engine still holds was composed into"
            )),
            FrameOutcome::Skipped(SkipReason::Validation)
        );
    }

    #[test]
    fn the_factory_is_built_without_touching_a_graphics_device() {
        // Making the factory must not enumerate adapters: it is built while the application is
        // still being described, and the device is opened by the first display instead.
        let _factory = factory(SharedGraphics::new(), Displays::new());
    }

    #[test]
    fn a_surface_no_loop_is_driving_is_refused_rather_than_drawn_nowhere() {
        // The map is empty until a loop writes into it, so this is the answer a factory installed
        // without the loop gets. The refusal comes before any adapter is asked for, which is why a
        // machine with no graphics device runs this test.
        let mut factory = factory(SharedGraphics::new(), Displays::new());
        let surface: Arc<dyn Surface> = Arc::new(OffscreenSurface::new(
            SurfaceId::new(1),
            Size::<DevicePx, Device>::new(DevicePx(64.0), DevicePx(64.0)),
        ));
        let target = RenderTarget::new(Size::new(64, 64), Scale::new(1.0));

        match factory(&surface, target) {
            Err(AppError::Platform(PlatformError::Backend(reason))) => assert!(
                reason.contains("would reach no screen"),
                "the refusal has to say where the frame would have gone, and it said: {reason}"
            ),
            Err(other) => panic!("a surface no loop is driving was refused with: {other}"),
            Ok(_) => panic!(
                "a surface no loop is driving has no display to reach, so a renderer for it would \
                 compose frames and put them nowhere"
            ),
        }
    }

    #[test]
    fn a_device_that_was_asked_for_no_extensions_leaves_every_display_copying() {
        // `SharedGraphics::new()` asks for no Vulkan device extension at all, which is how a
        // machine that grants every one of them reproduces a machine that grants none. A machine
        // with no usable adapter answers the same way for the other reason, and both answers mean
        // the same thing: the console still runs, and every display copies its frames.
        assert!(
            scanout_device(&SharedGraphics::new()).is_none(),
            "a device that enabled none of them was handed to the frame loop, which would set \
             every display up for images that device cannot make"
        );
    }

    #[test]
    fn a_device_that_grants_them_is_the_one_the_displays_are_built_on() {
        let test = "a_device_that_grants_them_is_the_one_the_displays_are_built_on";
        let graphics = SharedGraphics::with_extensions(zgui_platform_drm::EXTENSIONS.to_vec());
        let Some(gpu) = scanout_device(&graphics) else {
            eprintln!(
                "{test}: no adapter on this machine grants what an exported image needs, so \
                 nothing was asserted\n\
                 run it on a machine with a Vulkan driver to make it assert anything"
            );
            return;
        };

        for name in zgui_platform_drm::EXTENSIONS {
            assert!(
                gpu.vulkan_extensions().contains(&name),
                "{} was handed to the frame loop without {name:?}",
                gpu.describe()
            );
        }
        eprintln!("{test}: {} carries all of them", gpu.describe());
    }
}
