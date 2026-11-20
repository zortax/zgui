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
        /// Opened by the first display's renderer and shared by every display. A frame is composed
        /// into a texture of the renderer's own, read back and copied into a buffer the driver
        /// allocated.
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
            let graphics = compositor_wgpu::SharedGraphics::new();
            // No device is given to the loop for scanout images, so every display reads its frames
            // back and copies them into buffers the driver allocated.
            self.with_renderer(console::factory(graphics, displays.clone()))
                .run_on(
                    move |handler| zgui_platform_drm::run(handler, &displays, None),
                    view,
                )
        }
    }
}
