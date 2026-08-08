//! An application: a window, what is in it, and what draws it.

pub mod fonts;
mod graphics;

use zgui_platform::{AppHandler, PlatformError};
use zgui_runtime::AppError;
use zgui_view::{Anchor, BuildCx, IntoView, View};

pub use crate::app::fonts::Fonts;

/// A platform backend's entry point: it takes the application and drives it until it stops.
///
/// A windowing backend blocks until the last window closes; one over buffers may return at once,
/// which is what makes the same application runnable in a test.
pub type Driver = fn(Box<dyn AppHandler>) -> Result<(), PlatformError>;

/// The driver [`App::run`] uses: this machine's own desktop.
///
/// [`App::run_on`] takes a driver so that an application can be run somewhere other than a screen,
/// and the other reason to name one is to sit *between* the desktop and the application without
/// giving up either: a handler that wraps this one sees every event the window produces, and can
/// produce events of its own, while the window, the graphics device and the compositor are the
/// real ones.
///
/// ```no_run
/// use zgui::prelude::*;
///
/// # fn main() -> Result<(), zgui::Error> {
/// app().run_on(
///     |handler| zgui::app::desktop()(handler),
///     || view! { column() },
/// )
/// # }
/// ```
pub fn desktop() -> Driver {
    zgui_platform_winit::run
}

/// An application, before it runs. The same as [`App::new`], under a name a component cannot take.
///
/// This is what the prelude exports, and the prelude deliberately does **not** export the type.
/// `App` is the obvious name for a root component and for that component's scoped style sheet, and
/// a `style!` block declares a type of that name — so a prelude carrying a type called `App` would
/// be shadowed by the first application that named its root the obvious thing, and the entry point
/// would stop resolving in a program that never mentioned it. It cannot happen through this name:
/// `#[component]` refuses a name that does not begin with a capital, so nothing a component or its
/// style sheet declares can be spelled `app`.
///
/// The type is still there, at [`zgui::App`](App), for anything that has to name it.
///
/// ```no_run
/// use zgui::prelude::*;
///
/// # fn main() -> Result<(), zgui::Error> {
/// app().with_title("Counter").run(|| view! { column() })
/// # }
/// ```
pub fn app() -> App {
    App::new()
}

/// An application, before it runs.
///
/// Everything a program needs in order to be a program is already decided here — the window, the
/// graphics device, the font engine, the glyph rasteriser and the event loop — so an application
/// describes its interface and nothing else. Each of those decisions can still be taken back:
/// [`App::with_fonts`] replaces the faces, and [`App::run_on`] replaces the whole platform.
///
/// ```no_run
/// use zgui::prelude::*;
///
/// # fn main() -> Result<(), zgui::Error> {
/// app()
///     .with_title("Counter")
///     .with_size(360.0, 220.0)
///     .with_stylesheet("root { padding: 24px }")
///     .run(|| view! { column {text {"0"}} })
/// # }
/// ```
pub struct App {
    /// The application the frame loop sees.
    inner: zgui_runtime::App,
    /// The faces it draws with, once it has been asked or has run out of chances to say.
    ///
    /// Empty until then, so that an application shipping its own faces never enumerates the
    /// machine's: what is installed here is the largest thing a launch does before it has a
    /// window, and a builder that had already done it by the time [`App::with_fonts`] was reached
    /// would do it for nothing.
    fonts: std::cell::OnceCell<Fonts>,
    /// What draws its windows, when it is not this machine's own graphics device.
    renderer: Option<zgui_runtime::RendererFactory>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// A size in CSS pixels, which is the space every window measurement is written in.
fn css_size(width: f32, height: f32) -> zgui_geom::Size<zgui_geom::CssPx, zgui_geom::Css> {
    zgui_geom::Size::new(zgui_geom::CssPx(width), zgui_geom::CssPx(height))
}

impl App {
    /// An application with the window it launches with, the machine's own faces, and nothing in it.
    ///
    /// Every other window it opens is asked for while it runs, through
    /// [`use_windows`](zgui_runtime::use_windows).
    pub fn new() -> Self {
        Self {
            inner: zgui_runtime::App::new(),
            fonts: std::cell::OnceCell::new(),
            renderer: None,
        }
    }

    /// Names the window.
    pub fn with_title(mut self, title: impl Into<zgui_vocab::SharedString>) -> Self {
        self.inner = self.inner.with_title(title);
        self
    }

    /// Names the application to the desktop.
    ///
    /// This is what a compositor matches window rules, icons and task-bar grouping against — the
    /// Wayland toplevel's `app_id` and the X11 window's `WM_CLASS`. It is not the title: the title
    /// is what a user reads and follows the document, while this identifies the *program* and
    /// should match the `.desktop` file shipped beside it, so the convention is a reverse-domain
    /// name such as `dev.example.Counter`.
    ///
    /// An application that sets none is one the desktop cannot address: its windows carry an empty
    /// class, no rule can select them, and they group under nothing.
    pub fn with_application_id(mut self, id: impl Into<zgui_vocab::SharedString>) -> Self {
        self.inner = self.inner.with_application_id(id);
        self
    }

    /// Sets the size the window starts at, in CSS pixels.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.inner = self.inner.with_size(width, height);
        self
    }

    /// The smallest size the user may drag the window to, in CSS pixels.
    pub fn with_min_size(mut self, width: f32, height: f32) -> Self {
        self.inner.attributes_mut().min_size = Some(css_size(width, height));
        self
    }

    /// The largest size the user may drag the window to, in CSS pixels.
    pub fn with_max_size(mut self, width: f32, height: f32) -> Self {
        self.inner.attributes_mut().max_size = Some(css_size(width, height));
        self
    }

    /// Whether the user may resize the window at all.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.inner.attributes_mut().resizable = resizable;
        self
    }

    /// Whether the desktop draws the title bar and frame.
    ///
    /// An application that turns this off draws its own, and owes the user what the desktop's would
    /// have given them: somewhere to drag the window by, edges to resize from, and a way to close
    /// it. See [`WindowHandle::move_drag_handler`](zgui_runtime::WindowHandle::move_drag_handler).
    pub fn with_decorations(mut self, decorated: bool) -> Self {
        self.inner.attributes_mut().decorated = decorated;
        self
    }

    /// Whether the window may be partly transparent.
    ///
    /// What a window that draws its own rounded corners needs, so the desktop shows through them.
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.inner.attributes_mut().transparent = transparent;
        self
    }

    /// Where the window should open, measured from the desktop's origin.
    ///
    /// Ignored where a desktop places windows itself, which is every Wayland compositor.
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.inner.attributes_mut().position = Some(zgui_geom::Point::new(
            zgui_geom::CssPx(x),
            zgui_geom::CssPx(y),
        ));
        self
    }

    /// Whether the window should open maximised.
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.inner.attributes_mut().maximized = maximized;
        self
    }

    /// Whether the window should open full screen, and how.
    pub fn with_fullscreen(mut self, mode: Option<zgui_platform::FullscreenMode>) -> Self {
        self.inner.attributes_mut().fullscreen = mode;
        self
    }

    /// Where the window should sit in the desktop's stacking order.
    ///
    /// Ignored where a desktop does not let an application place itself in the stack.
    pub fn with_level(mut self, level: zgui_platform::WindowLevel) -> Self {
        self.inner.attributes_mut().level = level;
        self
    }

    /// The picture the desktop should show for the window.
    ///
    /// Ignored where the desktop takes it from elsewhere — from the desktop entry on Wayland, from
    /// the bundle on macOS. Those read
    /// [`App::with_application_id`](App::with_application_id) instead.
    pub fn with_icon(mut self, icon: zgui_platform::WindowIcon) -> Self {
        self.inner.attributes_mut().icon = Some(icon);
        self
    }

    /// A light or dark preference for this application's window, rather than the desktop's.
    pub fn with_theme(mut self, theme: zgui_platform::ColorScheme) -> Self {
        self.inner.attributes_mut().theme = Some(theme);
        self
    }

    /// Runs `setup` in the scope above every window, before the first one opens.
    ///
    /// Where state belonging to the *application* rather than to one window goes. A context
    /// provided inside a window's view is that window's own and no other window resolves it; one
    /// provided here is resolved by every window there will ever be.
    ///
    /// ```no_run
    /// use zgui::prelude::*;
    ///
    /// #[derive(Clone, Copy)]
    /// struct Count(RwSignal<i32>);
    ///
    /// # fn main() -> Result<(), zgui::Error> {
    /// app()
    ///     .with_context(|| provide_context(Count(RwSignal::new(0))))
    ///     .run(|| view! { column() })
    /// # }
    /// ```
    pub fn with_context(mut self, setup: impl FnOnce() + 'static) -> Self {
        self.inner = self.inner.with_context(setup);
        self
    }

    /// Decides when the application stops.
    ///
    /// The default stops it when the last window closes. See
    /// [`ExitPolicy`](zgui_runtime::ExitPolicy) for what else it can be, which matters as soon as
    /// an application opens more than one window.
    pub fn with_exit_policy(mut self, exit: zgui_runtime::ExitPolicy) -> Self {
        self.inner = self.inner.with_exit_policy(exit);
        self
    }

    /// Installs the application's own style sheet.
    ///
    /// It is added at the author origin, over this framework's own sheet, which is what gives
    /// every element name its layout before a single application rule is written.
    pub fn with_stylesheet(mut self, css: impl Into<String>) -> Self {
        self.inner = self.inner.with_stylesheet(css);
        self
    }

    /// Installs `probe`, which is told about every frame of every window this opens.
    ///
    /// The seam a diagnostic tool attaches to. It is what an inspector overlay is wired in through:
    /// a view sees the document, and what a probe is handed is the *frame* — the scene that was
    /// emitted, the damage that was answered, the geometry that was computed and what the renderer
    /// holds — none of which outlives the frame that produced it.
    ///
    /// ```no_run
    /// use std::cell::Cell;
    /// use std::rc::Rc;
    /// use zgui::prelude::*;
    /// use zgui::runtime::{FrameProbe, Window};
    ///
    /// /// Counts the frames the window ran.
    /// #[derive(Default)]
    /// struct Frames(Cell<u64>);
    ///
    /// impl FrameProbe for Frames {
    ///     fn frame_ended(&self, _window: &Window) {
    ///         self.0.set(self.0.get() + 1);
    ///     }
    /// }
    ///
    /// # fn main() -> Result<(), zgui::Error> {
    /// app()
    ///     .with_probe(Rc::new(Frames::default()))
    ///     .run(|| view! { column() })
    /// # }
    /// ```
    pub fn with_probe(mut self, probe: std::rc::Rc<dyn zgui_runtime::FrameProbe>) -> Self {
        self.inner = self.inner.with_probe(probe);
        self
    }

    /// Draws with `fonts` rather than with whatever this machine has installed.
    ///
    /// Said before [`App::fonts`] is asked for, this is also how an application declines to
    /// enumerate the machine's faces at all.
    pub fn with_fonts(mut self, fonts: Fonts) -> Self {
        self.fonts = std::cell::OnceCell::from(fonts);
        self
    }

    /// Draws through `factory` rather than through this machine's graphics device.
    ///
    /// What a window is drawn through is a decision, not a fact, and the two callers who need to
    /// take it are the one running on a device this framework has no backend for and the one
    /// running with no device at all — a test, which drives the same application over buffers and
    /// reads back what it drew.
    pub fn with_renderer(mut self, factory: zgui_runtime::RendererFactory) -> Self {
        self.renderer = Some(factory);
        self
    }

    /// The faces this application draws with, for registering one of its own.
    ///
    /// Asking settles the question [`App::with_fonts`] would otherwise answer: an application that
    /// has not named its own faces by the time it asks about them gets this machine's.
    pub fn fonts(&self) -> &Fonts {
        self.fonts.get_or_init(Fonts::system)
    }

    /// Opens the window, builds `view` into it, and runs until the last window closes.
    ///
    /// # Errors
    ///
    /// Returns whatever stopped it: a desktop that would not open a window, a machine with no
    /// usable graphics device, or an executor slot another asynchronous runtime already holds.
    pub fn run<F, V>(self, view: F) -> Result<(), AppError>
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        self.run_on(zgui_platform_winit::run, view)
    }

    /// The same, over a platform backend of the caller's choosing.
    ///
    /// The one thing an application cannot decide for itself is where it is running: a test drives
    /// the same application over buffers and a virtual clock, and nothing above this line changes.
    ///
    /// # Errors
    ///
    /// As [`App::run`].
    pub fn run_on<F, V, D>(self, driver: D, view: F) -> Result<(), AppError>
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
        D: FnOnce(Box<dyn AppHandler>) -> Result<(), PlatformError>,
    {
        self.into_handler(view)?.drive(driver)
    }

    /// Turns the application into the handler a platform backend drives, without driving it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::ForeignExecutor`] when another asynchronous runtime already holds this
    /// process's executor slot, because the reactive layer's tasks run on the thread that owns the
    /// window.
    pub fn into_handler<F, V>(self, mut view: F) -> Result<Handler, AppError>
    where
        F: FnMut() -> V + 'static,
        V: IntoView,
    {
        // The last point at which the application could have named its own faces has passed, so
        // this is where enumerating the machine's starts — on a thread of its own, alongside
        // opening the window and the graphics device rather than in front of them.
        let fonts = self.fonts.into_inner().unwrap_or_else(Fonts::system);
        let shaping = fonts.clone();
        let renderer = self.renderer.unwrap_or_else(graphics::factory);
        let runtime = self
            .inner
            .with_renderer(renderer)
            .with_metrics(Box::new({
                let fonts = fonts.clone();
                move || fonts.metrics()
            }))
            .with_text_engine(Box::new(move || {
                Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
            }))
            .with_glyph_raster(Box::new(move || fonts.raster()))
            // Installed by default beside the default renderer: an application with no surface
            // elements pays one virtual call per frame, and one with them needs no wiring.
            .with_embed(Box::new(|| Box::new(zgui_wgpu::WgpuSurfaces::new())))
            // The custom-element registry, for the same reason: an application that mounts none
            // pays two never-taken branches per frame.
            .with_custom(Box::new(|_document| zgui_custom::sources()))
            .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
                Box::new(view().into_view().build(cx))
            })?;
        Ok(Handler { runtime })
    }
}

/// A built application, waiting for a platform backend to drive it.
pub struct Handler {
    /// The frame loop.
    runtime: zgui_runtime::Runtime,
}

impl Handler {
    /// Hands the application to `driver` and returns when the loop finishes.
    ///
    /// # Errors
    ///
    /// Returns what the platform refused with, or what stopped the application while it ran — a
    /// window that could not be opened, or a machine with no usable graphics device.
    pub fn drive<D>(self, driver: D) -> Result<(), AppError>
    where
        D: FnOnce(Box<dyn AppHandler>) -> Result<(), PlatformError>,
    {
        // Taken before the runtime is handed over, because the driver consumes it.
        let failure = self.runtime.failure();
        driver(Box::new(self.runtime))?;
        failure.take().map_or(Ok(()), Err)
    }
}
