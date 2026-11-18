//! Running an application in a page.
//!
//! A page gives a program no thread to block. The graphics device opens through a promise, so the
//! start is a future: resolve the canvas, open the device, hand the application to the page. From
//! then on the page's callbacks drive it, and the function that started it has returned.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use zgui_platform::PlatformError;
use zgui_platform_web::Mount;
use zgui_render::Renderer;
use zgui_render_wgpu::{Canvas, GroupPrecision, PresentPacing, SharedGraphics, gpu::adapter};
use zgui_runtime::{AppError, RendererFactory};
use zgui_view::IntoView;

use crate::app::App;

/// An application that runs in a page.
///
/// Dropping the handle leaves the application running. [`Mounted::unmount`] stops it.
#[derive(Debug)]
#[must_use = "the handle is the way to stop the application"]
pub struct Mounted(());

impl Mounted {
    /// Stops the application and releases its canvas.
    ///
    /// The application receives its shutdown callback in the page's next task.
    pub fn unmount(self) {
        zgui_platform_web::stop();
    }
}

/// Starts `app` on `mount` and returns when the page drives it.
pub(crate) async fn mount<F, V>(app: App, mount: Mount, view: F) -> Result<Mounted, AppError>
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    // Before anything that can fail: a failure with no message is the hardest one to find.
    zgui_platform_web::report_panics();
    // Asked first: making the canvas of a second application would cover the first one.
    if zgui_platform_web::is_running() {
        return Err(AppError::Platform(PlatformError::Backend(
            "this page already runs an application".to_owned(),
        )));
    }
    let canvas = zgui_platform_web::canvas(&mount)?;
    let started = start_on(app, &mount, canvas.clone(), view).await;
    // A canvas the application made for itself goes when the start fails, so that it does not
    // cover what the page shows in its place.
    if started.is_err() && mount.owns_page() {
        canvas.remove();
    }
    started
}

/// Opens the graphics device for `canvas` and hands the application to the page.
async fn start_on<F, V>(
    mut app: App,
    mount: &Mount,
    canvas: zgui_platform_web::HtmlCanvasElement,
    view: F,
) -> Result<Mounted, AppError>
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    let mut recovery = None;
    if app.renderer.is_none() {
        let backends = backends();
        let graphics = SharedGraphics::open_async(
            backends,
            Canvas(canvas.clone()),
            zgui_platform_web::subpixel_text_suits(),
        )
        .await?;
        let precision = app.group_precision.unwrap_or_default();
        describe(&graphics);
        recovery = Some(Recovery {
            graphics: graphics.clone(),
            canvas: canvas.clone(),
            backends,
            running: false,
        });
        app.renderer = Some(factory(graphics, precision));
    }
    install_tasks();
    // A page has no file system, so the `src` of an image is a URL there.
    zgui_runtime::set_source_loader(std::rc::Rc::new(PageImages));
    let named_fonts = app.fonts.get().is_some();
    if load_page_fonts(&app).await == 0 && !named_fonts {
        zgui_platform_web::report(
            "zgui: this page has no font, so no text is drawn. A browser gives a page no \
             installed faces: preload a font file with \
             <link rel=\"preload\" as=\"fetch\" type=\"font/ttf\" href=\"…\" crossorigin>, \
             or register one with `Fonts::register`.",
        );
    }
    let runtime = app.into_handler(view)?.into_runtime();
    let failure = runtime.failure();
    zgui_platform_web::run(Box::new(runtime), canvas, mount.owns_page(), move || {
        // The runtime is gone with the driver. Whatever is left in the task pool goes now, and
        // so does the device: a loss after this would open a new one for no application.
        zgui_reactive::shutdown();
        RECOVERY.with(|held| held.borrow_mut().take());
        if let Some(error) = failure.take() {
            zgui_platform_web::report(&format!("zgui: {error}"));
        }
    })?;
    if let Some(recovery) = recovery {
        watch(&recovery.graphics);
        RECOVERY.with(|held| *held.borrow_mut() = Some(recovery));
        zgui_platform_web::on_context_change(|restored| {
            if restored {
                zgui_platform_web::spawn_local(recover());
            } else {
                zgui_platform_web::suspend();
            }
        });
    }
    Ok(Mounted(()))
}

/// Starts `app` on a canvas that fills the page, and reports a failure to the console.
pub(crate) fn start<F, V>(app: App, view: F)
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    zgui_platform_web::spawn_local(async move {
        if let Err(error) = mount(app, Mount::Viewport, view).await {
            zgui_platform_web::report(&format!("zgui: {error}"));
        }
    });
}

/// Says in the console which graphics interface the page draws with.
///
/// The two interfaces differ in what they can draw: WebGL 2 has no compute rasteriser and draws
/// text with one coverage for each pixel. Which one a browser gave is the first thing to know
/// when a page looks or runs unlike a native build.
fn describe(graphics: &SharedGraphics) {
    let Some(gpu) = graphics.gpu() else { return };
    let adapter = gpu.adapter().get_info();
    let capabilities = gpu.capabilities();
    zgui_platform_web::note(&format!(
        "zgui: drawing with {} on {} (vectors: {}, text: {})",
        if adapter.backend == zgui_render_wgpu::wgpu::Backend::Gl {
            "WebGL 2"
        } else {
            "WebGPU"
        },
        if adapter.name.is_empty() {
            "an unnamed adapter"
        } else {
            &adapter.name
        },
        if capabilities.vector_compute {
            "compute"
        } else {
            "coverage"
        },
        if capabilities.subpixel_text {
            "per-channel"
        } else {
            "greyscale"
        },
    ));
}

/// What a page needs to open a new graphics device after it lost one.
struct Recovery {
    /// The graphics every renderer of the page is made on.
    graphics: SharedGraphics,
    /// The canvas.
    canvas: zgui_platform_web::HtmlCanvasElement,
    /// The graphics interfaces to try.
    backends: zgui_render_wgpu::wgpu::Backends,
    /// Whether a recovery is under way.
    running: bool,
}

thread_local! {
    /// The recovery state of this page's application.
    static RECOVERY: std::cell::RefCell<Option<Recovery>> = const { std::cell::RefCell::new(None) };
}

/// Starts a recovery when the device of `graphics` is reported lost.
fn watch(graphics: &SharedGraphics) {
    if let Some(gpu) = graphics.gpu() {
        gpu.loss()
            .watch(|| zgui_platform_web::spawn_local(recover()));
    }
}

/// Replaces a lost graphics device.
///
/// A browser loses a device when its graphics process restarts, and some phones take it from a
/// page in the background. The application gives up its surfaces, a new device opens, and the
/// application opens its surfaces again on the same canvas. Each window is built again from its
/// view, as after a suspension on a phone.
async fn recover() {
    let taken = RECOVERY.with(|held| {
        let mut held = held.borrow_mut();
        let recovery = held.as_mut().filter(|recovery| !recovery.running)?;
        recovery.running = true;
        Some((
            recovery.graphics.clone(),
            recovery.canvas.clone(),
            recovery.backends,
        ))
    });
    let Some((graphics, canvas, backends)) = taken else {
        return;
    };
    zgui_platform_web::suspend();
    let subpixel_text = zgui_platform_web::subpixel_text_suits();
    match SharedGraphics::open_async(backends, Canvas(canvas), subpixel_text).await {
        Ok(fresh) => {
            if let Some(gpu) = fresh.gpu() {
                graphics.adopt(gpu);
                watch(&graphics);
            }
            zgui_platform_web::resume();
        }
        Err(error) => {
            zgui_platform_web::report(&format!("zgui: the graphics device was lost: {error}"));
        }
    }
    RECOVERY.with(|held| {
        if let Some(recovery) = held.borrow_mut().as_mut() {
            recovery.running = false;
        }
    });
}

/// Registers the font files the page preloads.
///
/// A browser gives a page no list of installed faces, so the page names the files: one preload
/// link for each. A file that does not load, or that is no font, is reported and left out.
/// Returns how many files were registered.
async fn load_page_fonts(app: &App) -> usize {
    let mut loaded = 0;
    for url in zgui_platform_web::page_fonts() {
        let registered = match zgui_platform_web::fetch(&url).await {
            Ok(bytes) => app
                .fonts()
                .register(Arc::new(bytes), None)
                .map_err(|error| format!("{url}: {error}")),
            Err(error) => Err(error),
        };
        match registered {
            Ok(()) => loaded += 1,
            Err(error) => {
                zgui_platform_web::report(&format!("zgui: a page font was not loaded: {error}"));
            }
        }
    }
    loaded
}

/// The graphics interfaces to try, in order.
///
/// WebGPU, then WebGL 2. The page's query string narrows the list: `?renderer=webgl2` or
/// `?renderer=webgpu`.
fn backends() -> zgui_render_wgpu::wgpu::Backends {
    zgui_platform_web::query("renderer").map_or_else(adapter::default_backends, |named| {
        adapter::parse_backends(&named)
    })
}

/// The renderer factory of a page: every canvas draws on the device that is already open.
fn factory(graphics: SharedGraphics, precision: GroupPrecision) -> RendererFactory {
    Box::new(move |surface, target| {
        let Some(canvas) = zgui_platform_web::canvas_of(surface.id()) else {
            return Err(AppError::Platform(PlatformError::NoSuchSurface));
        };
        // The page presents a canvas when the callback that drew it returns, so the display
        // paces every frame and nothing is told before a present.
        let mut renderer =
            graphics.renderer_for_surface(target, Canvas(canvas), PresentPacing::Display, None)?;
        renderer.set_group_precision(precision);
        zgui_render_vector_vello::attach(&mut renderer, target.size);
        Ok(Box::new(renderer) as Box<dyn Renderer>)
    })
}

/// The images of a page: the browser loads, decodes and scales each one.
struct PageImages;

impl zgui_runtime::SourceLoader for PageImages {
    fn size(&self, src: String) -> zgui_runtime::Loading<(u32, u32)> {
        Box::pin(async move { zgui_platform_web::image_size(&src).await })
    }

    fn pixels(
        &self,
        src: String,
        long_edge: u32,
    ) -> zgui_runtime::Loading<zgui_runtime::LoadedPixels> {
        Box::pin(async move {
            let (width, height, texels) = zgui_platform_web::image_pixels(&src, long_edge).await?;
            Ok(zgui_runtime::LoadedPixels {
                width,
                height,
                texels,
            })
        })
    }
}

/// Where background work runs in a page that has one thread.
struct PageTasks;

impl zgui_reactive::BackgroundSpawner for PageTasks {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        zgui_platform_web::spawn_local(future);
    }

    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send>) {
        // A task of its own, after the frame that asked for the work was painted.
        zgui_platform_web::defer(work);
    }
}

/// Routes background work to the page's own task queue.
fn install_tasks() {
    // Refused when something installed a spawner already, which then keeps the work.
    let _ = zgui_reactive::set_background_spawner(Arc::new(PageTasks));
}
