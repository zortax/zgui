//! Runs a zgui application on a bare Linux console, with no display server under it.
//!
//! Layer L8: product. Experimental, and kept in this repository: the console stack under it is not
//! published.
//!
//! [`RunDrm::run_drm`] is the entry point. It takes the place of [`App::run`](zgui::App::run):
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui_console::RunDrm;
//!
//! # fn main() -> Result<(), zgui::Error> {
//! app().run_drm(|| view! { column() })
//! # }
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod console;

#[cfg(target_os = "linux")]
pub use crate::run::RunDrm;

#[cfg(target_os = "linux")]
mod run {
    use zgui::App;
    use zgui_runtime::AppError;
    use zgui_view::IntoView;

    use crate::console;

    /// Runs an [`App`] on a Linux console.
    pub trait RunDrm {
        /// As [`App::run`], on a Linux console with no display server under it.
        ///
        /// Lights every display that is plugged in, builds `view` into the first, and runs until
        /// the application stops. The picture goes to the screen through the kernel and through
        /// nothing else: a mode on the display, the buffers that display scans out of, and a page
        /// flip per frame.
        ///
        /// The console backend and the renderer that draws through it are installed together,
        /// because they are correct together and useless apart. The loop holds a display's buffers
        /// and says which display a surface is; the renderer puts a composed frame in one. An
        /// application that installed one of them alone would draw frames that reach no screen.
        /// The map between them is made here and given to both.
        ///
        /// # The graphics device
        ///
        /// Opened here, before the displays exist, and by this method rather than by the first
        /// frame. A display can hand out the buffers it scans out of, so that a frame is copied
        /// into one on the graphics device and nothing is read back, and those buffers are images
        /// on the graphics device. They have to exist before the renderer that draws into them, so
        /// the device has to exist before the displays. One device is made here and given to the
        /// loop and to the renderer factory, the same way one map is.
        ///
        /// A machine where no adapter opens, or whose driver will not grant the Vulkan device
        /// extensions an exported image needs, still runs: every display keeps the copied path,
        /// where a frame is read back and copied into a buffer the driver allocated.
        ///
        /// **This needs the device.** Where a session daemon answers, it opens the card for this
        /// program and no privilege of its own is needed. Where none answers, this process opens
        /// the card and takes DRM master, which needs a free virtual terminal or root. It fails to
        /// start while a compositor holds the device.
        ///
        /// # Switching terminal
        ///
        /// A run holds the keyboard, so `Ctrl+Alt+Fn` reaches this program rather than the console
        /// driver. Where a session daemon answers, the chord asks that daemon for the terminal:
        /// the devices go on the way out, they are opened again on the way back, and every display
        /// is set to its mode again. Where none answers, no daemon owns the terminal, so the ask is
        /// refused and the reason reported once. A switch made from elsewhere, such as `chvt`,
        /// leaves such a run holding the display.
        ///
        /// # Errors
        ///
        /// As [`App::run`], and additionally whatever the console refused with: no device to open,
        /// a device this process cannot become master of, or a display that refused a buffer or a
        /// mode.
        fn run_drm<F, V>(self, view: F) -> Result<(), AppError>
        where
            F: FnMut() -> V + 'static,
            V: IntoView;
    }

    impl RunDrm for App {
        fn run_drm<F, V>(self, view: F) -> Result<(), AppError>
        where
            F: FnMut() -> V + 'static,
            V: IntoView,
        {
            let displays = zgui_platform_drm::Displays::new();
            // The extensions are asked for before the device is opened, because a device extension
            // can be enabled only while a device is created. Every device this graphics opens
            // reads the same list, so a device opened after a loss carries them too.
            let graphics = console::graphics();
            // `None` is the copied path. A machine that cannot make these images is an ordinary
            // machine rather than a failure to start.
            let gpu = console::scanout_device(&graphics);
            self.with_compositor_client(Box::new(console::driver))
                .with_renderer(console::factory(graphics, displays.clone()))
                .run_on(
                    move |handler| zgui_platform_drm::run(handler, &displays, gpu.as_deref()),
                    view,
                )
        }
    }
}
