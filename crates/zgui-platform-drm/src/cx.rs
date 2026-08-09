//! What the platform offers, over the displays that were discovered.

use std::cell::Cell;
use std::sync::Arc;

use zgui_platform::{
    Clipboard, Clock, ColorScheme, DecorationSource, MonitorInfo, PlatformCapabilities, PlatformCx,
    PlatformError, Surface, SurfaceAttributes, SurfaceId, Waker,
};

use crate::clipboard::ConsoleClipboard;

/// Everything the platform offers on a console: the displays, the clock and the wake channel.
///
/// On a console the surfaces exist first. A display is found when the device is opened, it has a
/// mode, and it goes on existing whether or not the application asks for it. So
/// [`PlatformCx::create_surface`] hands out a display that is already there, and refuses once they
/// are all taken.
///
/// The frame loop holds one of these and lends it to a callback, the way every other backend lends
/// its context. The three things that outlive a callback — a surface, the clock and the wake
/// channel — are shared handles and say so in their own types.
///
/// Everything here is a value something else built. The device stays with the frame loop, so this
/// context can be asserted on a machine with no `/dev/dri` at all.
///
/// ```
/// use std::sync::Arc;
///
/// use zgui_platform::{Clock, PlatformCx, SurfaceAttributes, Waker};
/// use zgui_platform_drm::{ConsoleClipboard, DrmCx, EventfdWaker, SystemClock};
///
/// let waker = Arc::new(EventfdWaker::new()?);
/// let cx = DrmCx::new(
///     Vec::new(),
///     Vec::new(),
///     Arc::new(SystemClock::new()) as Arc<dyn Clock>,
///     Arc::clone(&waker) as Arc<dyn Waker>,
///     ConsoleClipboard::new(waker as Arc<dyn Waker>),
/// );
///
/// assert_eq!(cx.claimed(), 0);
/// assert!(
///     cx.create_surface(&SurfaceAttributes::new("drm")).is_err(),
///     "a console with no display has none to hand out, and no window manager to open one"
/// );
/// # Ok::<(), zgui_platform::PlatformError>(())
/// ```
pub struct DrmCx {
    /// The clock every phase reads.
    clock: Arc<dyn Clock>,
    /// How another thread reaches this loop.
    waker: Arc<dyn Waker>,
    /// The clipboard, which holds nothing.
    clipboard: ConsoleClipboard,
    /// What this platform declares it can do.
    capabilities: PlatformCapabilities,
    /// One surface per display, in the order the displays were found.
    surfaces: Vec<Arc<dyn Surface>>,
    /// How many of them the application has asked for.
    ///
    /// They are handed out in order, so this is both the count and the index of the next one.
    claimed: Cell<usize>,
    /// What is known about each display, in the same order.
    monitors: Vec<MonitorInfo>,
    /// Whether the loop has been asked to finish.
    exiting: Cell<bool>,
}

impl DrmCx {
    /// Returns a context that serves `surfaces` over the displays `monitors` describes.
    ///
    /// The two lists are in the same order, one entry per display. `clock`, `waker` and `clipboard`
    /// are the console's own, and the clipboard answers a read through the same wake channel.
    ///
    /// The frame loop calls this. It opens the device, discovers the outputs, and turns them into
    /// the two lists with [`surface::one_per_output`](crate::surface::one_per_output) and
    /// [`output::describe`](crate::output::describe). Building them out there keeps the device out
    /// of this type: a context holds values, and it needs no device to hold them.
    pub fn new(
        surfaces: Vec<Arc<dyn Surface>>,
        monitors: Vec<MonitorInfo>,
        clock: Arc<dyn Clock>,
        waker: Arc<dyn Waker>,
        clipboard: ConsoleClipboard,
    ) -> Self {
        Self {
            clock,
            waker,
            clipboard,
            capabilities: capabilities(),
            surfaces,
            claimed: Cell::new(0),
            monitors,
            exiting: Cell::new(false),
        }
    }

    /// Returns how many displays the application has claimed.
    ///
    /// Displays are handed out in order, so this is the length of the prefix that anything draws
    /// into. The frame loop cuts its own surface list down to it: a display nothing asked for is
    /// left out of a frame, because a flip of an empty framebuffer would blank a screen the
    /// application never claimed.
    pub fn claimed(&self) -> usize {
        self.claimed.get()
    }
}

/// Written by hand, because the contract makes none of a surface, a clock or a wake channel
/// printable. It prints how many displays this context has and how many are taken.
impl core::fmt::Debug for DrmCx {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("DrmCx")
            .field("surfaces", &self.surfaces.len())
            .field("claimed", &self.claimed.get())
            .field("monitors", &self.monitors.len())
            .field("exiting", &self.exiting.get())
            .finish()
    }
}

impl PlatformCx for DrmCx {
    /// Returns the next display that nothing has claimed.
    ///
    /// The attributes are read for nothing: a title has nowhere to go, and a size is the mode's.
    fn create_surface(
        &self,
        _attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        let claimed = self.claimed.get();
        let Some(surface) = self.surfaces.get(claimed) else {
            // `SurfaceCreation` rather than `Unsupported`: this platform does create surfaces, and
            // what failed is this request. A caller that opened a second window is told that the
            // displays ran out, which is a thing it can report to a person.
            return Err(PlatformError::SurfaceCreation(format!(
                "every one of the {} display(s) this device drives is already in use, and a \
                 console has no window manager to open another",
                self.surfaces.len()
            )));
        };
        self.claimed.set(claimed + 1);
        Ok(Arc::clone(surface))
    }

    /// Does nothing: a display is not a window and does not go away.
    ///
    /// The surface stays in the set and stays claimed. Handing the same display to a second caller
    /// after the first let it go would give two callers one picture.
    fn destroy_surface(&self, _id: SurfaceId) {}

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.surfaces[..self.claimed.get()]
            .iter()
            .find(|surface| surface.id() == id)
            .map(Arc::clone)
    }

    /// Returns the surfaces that have been handed out.
    ///
    /// A display nothing asked for draws nothing, so it is left out of the set the application
    /// iterates.
    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.surfaces[..self.claimed.get()].to_vec()
    }

    /// Returns every display the device drives, claimed or not.
    fn monitors(&self) -> Vec<MonitorInfo> {
        self.monitors.clone()
    }

    /// Returns the first display that was found.
    ///
    /// The kernel names no primary display, so the choice is this backend's and it is the order the
    /// connectors came back in.
    fn primary_monitor(&self) -> Option<MonitorInfo> {
        self.monitors.first().cloned()
    }

    /// Returns nothing. A console states no preference, and absent means unknown: a caller that
    /// read it as light would be guessing.
    fn color_scheme(&self) -> Option<ColorScheme> {
        None
    }

    fn clipboard(&self) -> &dyn Clipboard {
        &self.clipboard
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock)
    }

    fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.waker)
    }

    fn request_exit(&self) {
        self.exiting.set(true);
    }

    fn is_exiting(&self) -> bool {
        self.exiting.get()
    }
}

/// Returns what a bare console can do.
///
/// Built up from nothing, so a capability nobody filled in reads as absent. Two of them are
/// answered:
///
/// * there are no clipboard representations at all. The starting set claims plain text, which every
///   desktop has and this console does not: there is no selection owner to ask, so a paste command
///   offered here would fail every time; and
/// * the decorations are the application's, because nothing else here draws anything. A title bar
///   on this backend exists only if the application draws one.
///
/// # Pointer confinement and pointer lock
///
/// `pointer_confine` and `pointer_lock` both stay false, and both are close. This backend owns the
/// pointer's position and already clamps it to the union of the displays, and every relative device
/// on it reports the pure motion a lock delivers.
///
/// What is missing is above this crate. The contract offers no method that asks for either, so a
/// component told yes would have nothing to call, and `PointerEvent` carries a position and no
/// movement, so a pointer held in place has nowhere to report what the device did. Declaring either
/// would offer a command that can never run. That is the half of [`PlatformCapabilities::none`]
/// that breaks, while the other half degrades. Both become this backend's question again on the day
/// the contract grows a way to ask.
///
/// `native_gestures` stays false and states something: this backend reports the raw pointer stream
/// and recognises no pinch, rotate or pan of its own, so the framework recognises them.
///
/// # The other fields
///
/// Every one keeps the empty answer, and each is true here. There is no window manager, so nothing
/// places a surface, stacks it, draws attention to it, or carries a drag to another application. A
/// display is not a window, so an overlay has no pop-up surface of its own. There is no input
/// method to steer, and `DrmSurface::set_text_input` reports that absence by doing nothing. And a
/// console states no light or dark preference, so the scheme is unknown.
fn capabilities() -> PlatformCapabilities {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.clipboard_formats = Vec::new();
    capabilities.decorations = DecorationSource::Application;
    capabilities
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::DrmCx;
    use crate::clipboard::ConsoleClipboard;
    use crate::clock::SystemClock;
    use crate::waker::EventfdWaker;
    use zgui_geom::{DevicePx, Point, Size};
    use zgui_platform::{
        ClipboardFormat, Clock, DecorationSource, MonitorInfo, PlatformCapabilities, PlatformCx,
        PlatformError, Surface, SurfaceAttributes, SurfaceId, Waker,
    };
    use zgui_platform_headless::OffscreenSurface;

    /// Returns a context over `surfaces` and the `monitors` that describe them.
    ///
    /// Nothing here opens a device. A context holds values something else built, so every answer it
    /// gives can be asserted on a machine with no `/dev/dri` at all.
    ///
    /// The clock, the wake channel and the clipboard are the console's own, so the backend's own
    /// state is what is exercised.
    fn console(surfaces: Vec<Arc<dyn Surface>>, monitors: Vec<MonitorInfo>) -> DrmCx {
        let waker = Arc::new(EventfdWaker::new().expect("a wake channel is openable"));
        let clipboard = ConsoleClipboard::new(Arc::clone(&waker) as Arc<dyn Waker>);
        DrmCx::new(
            surfaces,
            monitors,
            Arc::new(SystemClock::new()) as Arc<dyn Clock>,
            waker as Arc<dyn Waker>,
            clipboard,
        )
    }

    /// Returns a stand-in for a display, numbered `id`.
    ///
    /// This backend's own surface holds a device open, which is the one thing a test on a machine
    /// with no device cannot have. The headless backend's surface implements the same contract and
    /// needs nothing, so it stands in wherever the assertion is about the context.
    fn display(id: u64) -> Arc<dyn Surface> {
        Arc::new(OffscreenSurface::new(
            SurfaceId::new(id),
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
        ))
    }

    /// Returns what the contract knows about a display of `width` by `height`.
    fn monitor(width: f32, height: f32) -> MonitorInfo {
        MonitorInfo::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(height)),
            1.0,
        )
    }

    #[test]
    fn a_request_for_a_display_that_is_not_there_is_refused() {
        let cx = console(Vec::new(), Vec::new());
        let Err(refusal) = cx.create_surface(&SurfaceAttributes::new("drm")) else {
            panic!("a console with no display has no surface to hand out");
        };
        assert!(
            matches!(refusal, PlatformError::SurfaceCreation(_)),
            "the request to create a surface is what failed: {refusal:?}"
        );
        assert!(
            refusal.to_string().contains("0 display"),
            "the refusal says how many displays there were: {refusal}"
        );
    }

    #[test]
    fn a_request_past_the_last_display_is_refused_with_the_count() {
        let cx = console(vec![display(1)], Vec::new());
        cx.create_surface(&SurfaceAttributes::new("drm"))
            .expect("the one display this device drives is handed out");

        let Err(refusal) = cx.create_surface(&SurfaceAttributes::new("drm")) else {
            panic!("there is no second display to hand out");
        };
        assert!(
            refusal.to_string().contains("1 display"),
            "the refusal says how many displays there were: {refusal}"
        );
    }

    #[test]
    fn the_set_holds_what_was_handed_out() {
        let cx = console(vec![display(1), display(2)], Vec::new());
        assert!(
            cx.surfaces().is_empty(),
            "a display nothing asked for is not in the set"
        );
        assert_eq!(cx.claimed(), 0);

        let first = cx
            .create_surface(&SurfaceAttributes::new("drm"))
            .expect("the first display is handed out");
        assert_eq!(
            cx.surfaces().len(),
            1,
            "the display that was asked for is in the set, and the other one is not"
        );
        assert_eq!(cx.claimed(), 1);
        assert_eq!(first.id(), SurfaceId::new(1));

        let second = cx
            .create_surface(&SurfaceAttributes::new("drm"))
            .expect("the second display is handed out");
        assert_eq!(
            second.id(),
            SurfaceId::new(2),
            "displays are handed out in order"
        );
        assert_eq!(cx.surfaces().len(), 2);
        assert!(
            cx.create_surface(&SurfaceAttributes::new("drm")).is_err(),
            "and a refused request adds none"
        );
        assert_eq!(cx.surfaces().len(), 2);
        assert_eq!(cx.claimed(), 2);
    }

    #[test]
    fn a_display_is_found_by_the_number_it_was_handed_out_under() {
        let cx = console(vec![display(1), display(2)], Vec::new());
        let _ = cx.create_surface(&SurfaceAttributes::new("drm"));

        assert!(cx.surface(SurfaceId::new(1)).is_some());
        assert!(
            cx.surface(SurfaceId::new(2)).is_none(),
            "a display nothing asked for is not reachable by its number either"
        );
    }

    #[test]
    fn a_display_that_was_let_go_is_not_handed_out_again() {
        let cx = console(vec![display(1)], Vec::new());
        let _ = cx.create_surface(&SurfaceAttributes::new("drm"));
        cx.destroy_surface(SurfaceId::new(1));

        assert_eq!(
            cx.surfaces().len(),
            1,
            "a display is not a window and stays"
        );
        assert!(
            cx.create_surface(&SurfaceAttributes::new("drm")).is_err(),
            "and it is never handed to a second caller"
        );
    }

    #[test]
    fn a_console_with_no_display_reports_no_monitor() {
        let cx = console(Vec::new(), Vec::new());
        assert!(cx.monitors().is_empty());
        assert!(cx.primary_monitor().is_none());
    }

    #[test]
    fn the_first_display_that_was_found_is_the_primary_one() {
        let cx = console(
            Vec::new(),
            vec![monitor(1920.0, 1080.0), monitor(800.0, 600.0)],
        );
        assert_eq!(
            cx.monitors().len(),
            2,
            "an unclaimed display is still reported"
        );
        assert_eq!(
            cx.primary_monitor().expect("a display was found").size,
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
            "the kernel names no primary display, so the first one found is it"
        );
    }

    #[test]
    fn asking_the_loop_to_finish_is_observable() {
        let cx = console(Vec::new(), Vec::new());
        assert!(!cx.is_exiting());
        cx.request_exit();
        assert!(cx.is_exiting());
    }

    #[test]
    fn a_console_claims_no_clipboard_and_draws_its_own_decorations() {
        let cx = console(Vec::new(), Vec::new());
        let capabilities = cx.capabilities();
        assert!(
            !capabilities.supports_clipboard_format(ClipboardFormat::Text),
            "there is no selection owner to ask, so not even text works"
        );
        assert!(capabilities.decorations.is_application());
        assert!(!capabilities.system_color_scheme);
        assert_eq!(cx.color_scheme(), None);
        assert!(!capabilities.native_popup_surfaces);
        assert!(!capabilities.ime);
    }

    #[test]
    fn a_console_owns_its_pointer_and_claims_neither_confinement_nor_a_lock() {
        // Both are a real question now that there is a pointer. This backend owns the position and
        // already clamps it to the union of the displays, and every relative device on it reports
        // pure motion — so what is missing is above this crate: the contract has no method that
        // asks for either, and a pointer event carries a position and no movement. A component
        // told yes would find nothing to call and nowhere for the answer to arrive.
        let cx = console(Vec::new(), Vec::new());
        let capabilities = cx.capabilities();

        assert!(!capabilities.pointer_confine);
        assert!(!capabilities.pointer_lock);
        assert!(
            !capabilities.native_gestures,
            "and a pinch is the framework's to recognise, because this reports the raw stream"
        );
    }

    #[test]
    fn a_console_declares_the_two_things_it_answers_and_keeps_every_other_answer_empty() {
        // The whole set at once, so that a field flipped here has to be meant. Building up from
        // nothing makes a capability nobody filled in read as absent, which degrades. Building down
        // from everything would make the same omission read as present, which breaks.
        let cx = console(Vec::new(), Vec::new());

        let mut answered = PlatformCapabilities::none();
        answered.clipboard_formats = Vec::new();
        answered.decorations = DecorationSource::Application;
        assert_eq!(
            cx.capabilities(),
            &answered,
            "a console answers the clipboard and the decorations, and nothing else"
        );
    }
}
