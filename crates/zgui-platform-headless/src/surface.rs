//! A surface that is a buffer, with no window behind it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{
    CursorStyle, FullscreenMode, Surface, SurfaceAttributes, SurfaceId, TextInput,
};

/// A surface that is a buffer, with no window behind it.
///
/// Everything a real surface would do to a window is recorded instead, because a headless backend
/// exists to be asserted against. A test that cannot see whether a frame was asked for cannot
/// check the loop's parking, and the parking is what the whole of this backend is for.
///
/// A surface never draws itself: it counts the requests and hands the drawing to whoever answers
/// [`SurfaceEvent::RedrawRequested`](zgui_platform::SurfaceEvent::RedrawRequested).
#[derive(Debug)]
pub struct OffscreenSurface {
    /// Which surface this is.
    id: SurfaceId,
    /// How large the buffer is, in physical pixels.
    size: Mutex<Size<DevicePx, Device>>,
    /// How many physical pixels there are to a CSS pixel.
    scale_factor: Mutex<f64>,
    /// How fast the output this surface is on refreshes, in thousandths of a hertz.
    ///
    /// Zero means the platform does not say, which is what a backend that knows no rate reports.
    /// It is settable because how often a window answers a configure, and how often it ticks an
    /// animation, are both derived from it — so a suite that cannot move it cannot tell a loop that
    /// reads the output from one that assumes sixty hertz, and those two differ by a factor of four
    /// on the displays where the difference is felt.
    refresh_rate_millihertz: AtomicU32,
    /// How many frames have been asked for and not yet delivered.
    pending: AtomicU64,
    /// How many have been asked for in total.
    requested: AtomicU64,
    /// How many accessibility updates have been published.
    a11y_updates: AtomicU64,
    /// Every accessibility update published, in order, kept so a test can read what a screen
    /// reader would have been told.
    ///
    /// A count alone cannot tell an update that carried the right tree from one that carried an
    /// empty one, and the tree is the whole point of the channel. The whole sequence is kept and
    /// not only the last, because an update is a *difference*: what it names is resolved against
    /// everything sent before it, so a check on one update alone would reject every correct
    /// incremental one.
    a11y_log: Mutex<Vec<TreeUpdate>>,
    /// Everything the surface was told about text input, in order.
    ///
    /// Whether an input method is wanted at all, and where its candidate window goes, is a state
    /// the surface is *told*: nothing about the document says it, so a test that cannot read what
    /// was said cannot tell a window that reports the caret from one that reports nothing and
    /// leaves every Japanese keyboard unable to type into it.
    text_input_log: Mutex<Vec<Option<TextInput>>>,
    /// Whether the surface has been shown.
    visible: AtomicBool,
    /// The last title it was given.
    title: Mutex<String>,
    /// What the surface was asked to be when it was created.
    ///
    /// Kept because most of a request is answered by the desktop and never read back: what a test
    /// can otherwise check about an application identifier, a window level or an icon is nothing at
    /// all, and an attribute nothing reads is an attribute that silently stops being sent.
    requested_attributes: Mutex<SurfaceAttributes>,
}

impl OffscreenSurface {
    /// A surface of `size`, hidden, with nothing recorded yet.
    pub fn new(id: SurfaceId, size: Size<DevicePx, Device>) -> Self {
        Self {
            id,
            size: Mutex::new(size),
            scale_factor: Mutex::new(1.0),
            refresh_rate_millihertz: AtomicU32::new(0),
            pending: AtomicU64::new(0),
            requested: AtomicU64::new(0),
            a11y_updates: AtomicU64::new(0),
            a11y_log: Mutex::new(Vec::new()),
            text_input_log: Mutex::new(Vec::new()),
            visible: AtomicBool::new(false),
            title: Mutex::new(String::new()),
            requested_attributes: Mutex::new(SurfaceAttributes::default()),
        }
    }

    /// Records what the surface was asked to be.
    pub fn set_requested_attributes(&self, attributes: &SurfaceAttributes) {
        *self
            .requested_attributes
            .lock()
            .expect("the attributes are not poisoned") = attributes.clone();
    }

    /// What the surface was asked to be when it was created.
    pub fn requested_attributes(&self) -> SurfaceAttributes {
        self.requested_attributes
            .lock()
            .expect("the attributes are not poisoned")
            .clone()
    }

    /// How many frames have been asked for since this surface was created.
    pub fn redraws_requested(&self) -> u64 {
        self.requested.load(Ordering::Relaxed)
    }

    /// Whether a frame has been asked for and not yet delivered.
    pub fn has_pending_redraw(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }

    /// Takes the pending request, reporting whether there was one.
    ///
    /// Coalescing lives here: a hundred requests between two frames are one pending request and
    /// therefore one frame, which is exactly what the contract promises.
    pub fn take_pending_redraw(&self) -> bool {
        self.pending.swap(0, Ordering::Relaxed) > 0
    }

    /// How many accessibility updates have been published.
    pub fn a11y_updates(&self) -> u64 {
        self.a11y_updates.load(Ordering::Relaxed)
    }

    /// The last accessibility update published, if one has been.
    pub fn last_a11y_update(&self) -> Option<TreeUpdate> {
        self.a11y_log
            .lock()
            .expect("the log is not poisoned")
            .last()
            .cloned()
    }

    /// Every accessibility update published, in the order they were published.
    ///
    /// This is what a consumer holds: each update is applied over the ones before it, so a
    /// question about what the consumer can resolve is a question about the whole sequence.
    pub fn a11y_log(&self) -> Vec<TreeUpdate> {
        self.a11y_log
            .lock()
            .expect("the log is not poisoned")
            .clone()
    }

    /// Everything the surface was told about text input, in the order it was told.
    pub fn text_input_log(&self) -> Vec<Option<TextInput>> {
        self.text_input_log
            .lock()
            .expect("the log is not poisoned")
            .clone()
    }

    /// What the surface was last told about text input, if it has been told anything.
    ///
    /// The outer option is whether it was ever told; the inner one is what it was told — `None`
    /// being "no text is being typed here", which is a thing that has to be said.
    pub fn last_text_input(&self) -> Option<Option<TextInput>> {
        self.text_input_log
            .lock()
            .expect("the log is not poisoned")
            .last()
            .cloned()
    }

    /// Whether the surface has been shown.
    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    /// The title the surface was last given.
    pub fn title(&self) -> String {
        self.title
            .lock()
            .expect("the title is not poisoned")
            .clone()
    }

    /// Resizes the buffer, as a window manager would.
    pub fn resize(&self, size: Size<DevicePx, Device>) {
        *self.size.lock().expect("the size is not poisoned") = size;
    }

    /// Changes the scale the surface is presented at, as moving to another output would.
    pub fn set_scale_factor(&self, scale_factor: f64) {
        *self.scale_factor.lock().expect("the scale is not poisoned") = scale_factor;
    }

    /// Puts the surface on an output that refreshes at `millihertz`, or on one that does not say.
    ///
    /// Dragging a window from a seventy-five hertz panel onto a two-hundred-and-forty hertz one is
    /// this call, and the two are four times apart in everything paced against the output.
    pub fn set_refresh_rate_millihertz(&self, millihertz: Option<u32>) {
        self.refresh_rate_millihertz
            .store(millihertz.unwrap_or(0), Ordering::Relaxed);
    }
}

impl Surface for OffscreenSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn size(&self) -> Size<DevicePx, Device> {
        *self.size.lock().expect("the size is not poisoned")
    }

    fn scale_factor(&self) -> f64 {
        *self.scale_factor.lock().expect("the scale is not poisoned")
    }

    fn refresh_rate_millihertz(&self) -> Option<u32> {
        match self.refresh_rate_millihertz.load(Ordering::Relaxed) {
            0 => None,
            rate => Some(rate),
        }
    }

    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        let scale = self.scale_factor();
        let taken = Size::new(
            DevicePx(size.width.0 * scale as f32),
            DevicePx(size.height.0 * scale as f32),
        );
        self.resize(taken);
        Some(taken)
    }

    fn set_min_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn set_max_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn request_redraw(&self) {
        self.pending.store(1, Ordering::Relaxed);
        self.requested.fetch_add(1, Ordering::Relaxed);
    }

    fn pre_present_notify(&self) {}

    fn set_title(&self, title: &str) {
        *self.title.lock().expect("the title is not poisoned") = title.to_owned();
    }

    fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }

    fn set_decorated(&self, _decorated: bool) {}

    fn set_resizable(&self, _resizable: bool) {}

    fn set_maximized(&self, _maximized: bool) {}

    fn set_minimized(&self, _minimized: bool) {}

    fn set_fullscreen(&self, _mode: Option<FullscreenMode>) {}

    fn set_cursor(&self, _cursor: CursorStyle) {}

    fn set_text_input(&self, state: Option<TextInput>) {
        self.text_input_log
            .lock()
            .expect("the log is not poisoned")
            .push(state);
    }

    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        // A headless surface always has a listener, because whatever is driving it is the listener.
        // What it was told is kept rather than discarded: an assertion about the tree is the only
        // one that can tell a channel that carries the document from one that carries nothing.
        let update = build();
        self.a11y_log
            .lock()
            .expect("the log is not poisoned")
            .push(update);
        self.a11y_updates.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::OffscreenSurface;
    use zgui_geom::{DevicePx, Size};
    use zgui_platform::{Surface, SurfaceId};

    /// A surface four hundred pixels square.
    fn surface() -> OffscreenSurface {
        OffscreenSurface::new(
            SurfaceId::new(1),
            Size::new(DevicePx(400.0), DevicePx(400.0)),
        )
    }

    #[test]
    fn a_hundred_requests_between_two_frames_are_one_frame() {
        let surface = surface();
        for _ in 0..100 {
            surface.request_redraw();
        }
        assert!(surface.take_pending_redraw());
        assert!(!surface.take_pending_redraw());
        assert_eq!(
            surface.redraws_requested(),
            100,
            "the requests are still counted; it is the frames that coalesce"
        );
    }

    #[test]
    fn an_output_that_says_nothing_about_its_rate_is_told_apart_from_one_that_does() {
        let surface = surface();
        assert_eq!(
            surface.refresh_rate_millihertz(),
            None,
            "a backend that knows no rate must not invent one"
        );
        surface.set_refresh_rate_millihertz(Some(240_000));
        assert_eq!(surface.refresh_rate_millihertz(), Some(240_000));
        surface.set_refresh_rate_millihertz(None);
        assert_eq!(surface.refresh_rate_millihertz(), None);
    }

    #[test]
    fn a_surface_starts_hidden_and_is_shown_only_when_asked() {
        let surface = surface();
        assert!(!surface.is_visible());
        surface.set_visible(true);
        assert!(surface.is_visible());
    }
}
