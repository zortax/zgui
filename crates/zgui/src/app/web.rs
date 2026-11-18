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
pub(crate) async fn mount<F, V>(mut app: App, mount: Mount, view: F) -> Result<Mounted, AppError>
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    let canvas = zgui_platform_web::canvas(&mount)?;
    if app.renderer.is_none() {
        let graphics = SharedGraphics::open_async(backends(), Canvas(canvas.clone())).await?;
        let precision = app.group_precision.unwrap_or_default();
        app.renderer = Some(factory(graphics, precision));
    }
    install_tasks();
    load_page_fonts(&app).await;
    let runtime = app.into_handler(view)?.into_runtime();
    let failure = runtime.failure();
    zgui_platform_web::run(Box::new(runtime), canvas, mount.owns_page(), move || {
        // The runtime is gone with the driver. Whatever is left in the task pool goes now.
        zgui_reactive::shutdown();
        if let Some(error) = failure.take() {
            zgui_platform_web::report(&format!("zgui: {error}"));
        }
    })?;
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

/// Registers the font files the page preloads.
///
/// A browser gives a page no list of installed faces, so the page names the files: one preload
/// link for each. A file that does not load, or that is no font, is reported and left out.
async fn load_page_fonts(app: &App) {
    for url in zgui_platform_web::page_fonts() {
        let registered = match zgui_platform_web::fetch(&url).await {
            Ok(bytes) => app
                .fonts()
                .register(Arc::new(bytes), None)
                .map_err(|error| format!("{url}: {error}")),
            Err(error) => Err(error),
        };
        if let Err(error) = registered {
            zgui_platform_web::report(&format!("zgui: a page font was not loaded: {error}"));
        }
    }
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
