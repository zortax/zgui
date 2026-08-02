//! The borrowed platform context, valid only inside a callback.

use std::sync::Arc;

use crate::capabilities::PlatformCapabilities;
use crate::clipboard::Clipboard;
use crate::clock::Clock;
use crate::error::PlatformError;
use crate::monitor::MonitorInfo;
use crate::scroll::ScrollSettings;
use crate::surface::{Surface, SurfaceAttributes, SurfaceId};
use crate::theme::ColorScheme;
use crate::waker::Waker;

/// Everything the platform offers, for the duration of one callback.
///
/// It is passed in and never stored. That restriction is not a style preference: on every platform
/// with a real event loop, the object that can create windows and read monitors is valid only
/// while the loop is inside a callback, and is neither shareable nor sendable. Mirroring that
/// exactly here means the contract can be implemented on those platforms without a lie, and means
/// a caller cannot write code that happens to work on the one backend that would have tolerated
/// keeping it.
///
/// The three things that genuinely outlive a callback say so in their own types: a
/// [`Surface`], a [`Waker`] and a [`Clock`] are each shared, thread-safe and holdable. Everything
/// else is borrowed.
pub trait PlatformCx {
    /// Creates a surface.
    ///
    /// The surface is created hidden and is shown once its first frame has been drawn.
    fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError>;

    /// The surface with this identifier, while it still exists.
    fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>>;

    /// Every surface that currently exists.
    fn surfaces(&self) -> Vec<Arc<dyn Surface>>;

    /// Every output the platform knows about.
    fn monitors(&self) -> Vec<MonitorInfo>;

    /// The output the desktop considers primary, when it names one.
    fn primary_monitor(&self) -> Option<MonitorInfo>;

    /// The desktop's light or dark preference, when it can be discovered.
    ///
    /// Absent means unknown, not light. A platform that cannot be asked returns nothing rather
    /// than a guess, because guessing wrong shows every user who chose dark a white flash.
    fn color_scheme(&self) -> Option<ColorScheme>;

    /// The clipboards.
    fn clipboard(&self) -> &dyn Clipboard;

    /// What this platform can and cannot do.
    fn capabilities(&self) -> &PlatformCapabilities;

    /// What a scroll from this desktop's devices means.
    ///
    /// Defaulted rather than required, because a backend that has not been taught the difference
    /// should behave like an ordinary desktop rather than fail to compile — and because the answer
    /// a backend gives is a statement about a machine, which a backend with no machine under it
    /// cannot make. See [`scroll`](crate::scroll) for the sign convention every one of these values
    /// is expressed in.
    fn scroll_settings(&self) -> ScrollSettings {
        ScrollSettings::default()
    }

    /// Where the time comes from.
    ///
    /// Shared rather than borrowed, for the same reason the waker is: a timer heap and an
    /// animation driver both hold the clock for the life of the application and read it from
    /// inside every phase of every frame. A borrow valid only inside one callback would force each
    /// of them to keep a reading instead, and a held *reading* is a frozen clock — deadlines
    /// computed against it never arrive.
    fn clock(&self) -> Arc<dyn Clock>;

    /// A handle that wakes the loop from another thread.
    ///
    /// Shared rather than borrowed, because the whole purpose of it is to be kept by something
    /// that outlives this callback and does not run on this thread.
    fn waker(&self) -> Arc<dyn Waker>;

    /// Asks the loop to finish.
    ///
    /// Nothing stops at once: the loop finishes what it is doing, reports that it is shutting
    /// down, and then returns.
    fn request_exit(&self);

    /// Whether the loop has been asked to finish.
    fn is_exiting(&self) -> bool;
}
