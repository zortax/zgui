//! What the platform offers, over the displays that were discovered.

use std::cell::Cell;
use std::sync::Arc;

use zgui_geom::{DevicePx, Point, Size};
use zgui_platform::{
    Clipboard, Clock, ColorScheme, DecorationSource, MonitorInfo, PlatformCapabilities, PlatformCx,
    PlatformError, Surface, SurfaceAttributes, SurfaceId, Waker,
};

use crate::clipboard::ConsoleClipboard;
use crate::clock::SystemClock;
use crate::output::Output;
use crate::surface::DrmSurface;
use crate::waker::EventfdWaker;

/// Everything the platform offers on a console: the displays, the clock and the wake channel.
///
/// The difference from a desktop backend is that the surfaces exist first. A display is found when
/// the device is opened, it has a mode, and it goes on existing whether or not the application asks
/// for it — so [`PlatformCx::create_surface`] hands out a display that is already there, and refuses
/// once they are all taken.
///
/// It is held by the frame loop and lent to a callback, the same way every other backend lends its
/// context. The three things that outlive a callback — a surface, the clock and the wake channel —
/// are shared handles and say so in their own types.
#[derive(Debug)]
pub struct DrmCx {
    /// The clock every phase reads.
    clock: Arc<SystemClock>,
    /// How another thread reaches this loop.
    waker: Arc<EventfdWaker>,
    /// The clipboard, which holds nothing.
    clipboard: ConsoleClipboard,
    /// What this platform declares it can do.
    capabilities: PlatformCapabilities,
    /// One surface per display, in the order the displays were found.
    surfaces: Vec<Arc<DrmSurface>>,
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
    /// A context over `outputs`, with a surface ready for each of them.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the wake channel cannot be opened, which is a
    /// process that has run out of descriptors.
    pub fn new(outputs: Vec<Output>) -> Result<Self, PlatformError> {
        let waker = Arc::new(EventfdWaker::new()?);
        let monitors = outputs
            .iter()
            .map(|output| {
                describe(
                    output.mode.width(),
                    output.mode.height(),
                    output.mode.refresh_rate_millihertz(),
                )
            })
            .collect();

        // Numbered from one, in the order the displays were found, and never reused: a display is
        // never destroyed, so nothing frees a number for a second surface to take.
        let surfaces = (1..)
            .zip(outputs)
            .map(|(id, output)| Arc::new(DrmSurface::new(SurfaceId::new(id), output)))
            .collect();

        Ok(Self {
            clock: Arc::new(SystemClock::new()),
            clipboard: ConsoleClipboard::new(Arc::clone(&waker) as Arc<dyn Waker>),
            waker,
            capabilities: capabilities(),
            surfaces,
            claimed: Cell::new(0),
            monitors,
            exiting: Cell::new(false),
        })
    }

    /// The wake channel, as itself.
    ///
    /// The frame loop parks on its descriptor beside the device, and drains it after every wake.
    pub fn wake_channel(&self) -> &EventfdWaker {
        &self.waker
    }

    /// The surfaces that have been handed out, as themselves.
    ///
    /// The same set [`PlatformCx::surfaces`] answers, seen as this backend's own type: the frame
    /// loop needs the pipe to commit to and the redraw flag to take, and the contract offers
    /// neither. A display nothing asked for is left out here as well, because nothing draws into
    /// it and a flip of an empty framebuffer would blank a screen the application never claimed.
    pub fn drm_surfaces(&self) -> &[Arc<DrmSurface>] {
        &self.surfaces[..self.claimed.get()]
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
        Ok(Arc::clone(surface) as Arc<dyn Surface>)
    }

    /// Does nothing: a display is not a window and does not go away.
    ///
    /// The surface stays in the set and stays claimed. Handing the same display to a second caller
    /// after the first let it go would give two callers one picture.
    fn destroy_surface(&self, _id: SurfaceId) {}

    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.surfaces[..self.claimed.get()]
            .iter()
            .find(|surface| Surface::id(surface.as_ref()) == id)
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
    }

    /// Returns the surfaces that have been handed out.
    ///
    /// A display nothing asked for draws nothing, so it is left out of the set the application
    /// iterates.
    fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.surfaces[..self.claimed.get()]
            .iter()
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
            .collect()
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
        Arc::clone(&self.clock) as Arc<dyn Clock>
    }

    fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.waker) as Arc<dyn Waker>
    }

    fn request_exit(&self) {
        self.exiting.set(true);
    }

    fn is_exiting(&self) -> bool {
        self.exiting.get()
    }
}

/// What is known about one display, from the mode it is driven at.
///
/// The position is the origin for every display. A console has no desktop coordinate space: each
/// display is driven from its own framebuffer and nothing arranges them, so a layout here would be
/// this backend's invention rather than the machine's arrangement.
///
/// The scale factor is one, for the same reason a surface reports one: nothing states a scale, and
/// an invented one would size every application differently on every machine.
///
/// The name is left absent. The kernel calls a display after the kind of socket it is plugged into,
/// and an [`Output`] carries the connector's number rather than its kind, so there is nothing here
/// to name it with.
fn describe(width: u32, height: u32, millihertz: u32) -> MonitorInfo {
    let monitor = MonitorInfo::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(width as f32), DevicePx(height as f32)),
        1.0,
    );
    // A rate of zero is a mode whose timings give none, and inventing sixty here would hide that
    // from the fallback the contract states once and applies everywhere.
    if millihertz > 0 {
        monitor.with_refresh_rate_millihertz(millihertz)
    } else {
        monitor
    }
}

/// Returns what a bare console can do.
///
/// Built up from nothing, so a capability nobody filled in reads as absent. Two of them are
/// answered:
///
/// * there are no clipboard representations at all, because there is no selection owner to ask —
///   the starting set claims plain text, which every desktop has and this console does not; and
/// * the decorations are the application's, because nothing else here draws anything. A title bar
///   on this backend exists only if the application draws one.
fn capabilities() -> PlatformCapabilities {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.clipboard_formats = Vec::new();
    capabilities.decorations = DecorationSource::Application;
    capabilities
}

#[cfg(test)]
mod tests {
    use super::{DrmCx, describe};
    use zgui_geom::{DevicePx, Point, Size};
    use zgui_platform::{ClipboardFormat, PlatformCx, PlatformError, SurfaceAttributes};

    /// A context over a device with no display plugged in.
    fn console() -> DrmCx {
        DrmCx::new(Vec::new()).expect("a context with no display is still buildable")
    }

    #[test]
    fn a_request_for_a_display_that_is_not_there_is_refused() {
        let cx = console();
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
    fn the_set_holds_what_was_handed_out() {
        let cx = console();
        assert!(
            cx.surfaces().is_empty(),
            "a display nothing asked for is not in the set"
        );
        let _ = cx.create_surface(&SurfaceAttributes::new("drm"));
        assert!(cx.surfaces().is_empty(), "and a refused request adds none");
    }

    #[test]
    fn a_display_is_described_from_the_mode_it_runs_at() {
        let monitor = describe(1920, 1080, 60_000);
        assert_eq!(
            monitor.size,
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
            "a display is the extent of its mode"
        );
        assert_eq!(monitor.refresh_rate_millihertz, Some(60_000));
        assert_eq!(monitor.scale_factor, 1.0);
        assert_eq!(monitor.position, Point::new(DevicePx(0.0), DevicePx(0.0)));
        assert_eq!(monitor.name, None);
    }

    #[test]
    fn a_mode_whose_timings_give_no_rate_reports_none() {
        assert_eq!(describe(1920, 1080, 0).refresh_rate_millihertz, None);
    }

    #[test]
    fn a_device_with_no_display_reports_no_monitor() {
        let cx = console();
        assert!(cx.monitors().is_empty());
        assert!(cx.primary_monitor().is_none());
    }

    #[test]
    fn asking_the_loop_to_finish_is_observable() {
        let cx = console();
        assert!(!cx.is_exiting());
        cx.request_exit();
        assert!(cx.is_exiting());
    }

    #[test]
    fn a_console_claims_no_clipboard_and_draws_its_own_decorations() {
        let cx = console();
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
}
