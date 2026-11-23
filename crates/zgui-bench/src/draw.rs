//! What a window draws through, and what records the frames it drew.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use compositor_api::Renderer;
use compositor_types::{BufferAge, RenderTarget};
use compositor_wgpu::WgpuRenderer;
use zgui::platform::Surface;
use zgui::runtime::{AppError, RendererFactory};
use zgui_bench::renderers::NullRenderer;
use zgui_testkit_compositor::{Screen, Shown};

use crate::verify::FullFrame;

thread_local! {
    /// Where the pixel comparison leaves the readback it asked for.
    pub(crate) static CAPTURE: Arc<crate::verify::Capture> = Arc::default();

    /// Where each window's frame recorder is left for its opener to pick up, oldest first.
    ///
    /// A recorder is built with the application that opens the window, so this is the only place
    /// the two can be introduced. A slot rather than a list would hold only the window opened
    /// last, and the differential opens two.
    static FULL_FRAMES: RefCell<Vec<Rc<FullFrame>>> = const { RefCell::new(Vec::new()) };

    /// Whether the windows built from now on draw every frame whole and record it.
    static WHOLE: Cell<bool> = const { Cell::new(false) };
}

/// Whether the next frame a window draws is drawn over the whole output, read on the thread that
/// draws it.
static NEXT_WHOLE: AtomicBool = AtomicBool::new(false);

/// Makes the next frame a window draws cover the whole output.
pub(crate) fn next_whole() {
    NEXT_WHOLE.store(true, Ordering::Relaxed);
}

/// Returns the age a renderer whose kept target is `kept` reports for the next frame.
///
/// It is `Unknown` when [`next_whole`] asked for a whole frame, and `kept` otherwise.
pub(crate) fn age(kept: BufferAge) -> BufferAge {
    if NEXT_WHOLE.load(Ordering::Relaxed) {
        BufferAge::Unknown
    } else {
        kept
    }
}

/// Notes that a window drew a frame, which answers [`next_whole`].
pub(crate) fn drew() {
    NEXT_WHOLE.store(false, Ordering::Relaxed);
}

/// Makes the windows built from now on draw every frame over the whole output and record it, or
/// stop doing so.
///
/// A comparison of two windows reads the picture each of them shows. A measurement leaves this
/// off, because a whole frame and its transcript cost what the measurement is timing.
pub(crate) fn draw_whole(on: bool) {
    WHOLE.with(|whole| whole.set(on));
}

/// The frame recorder of the application that was built most recently.
///
/// # Panics
///
/// Panics when no application has been built since the last call, because a comparison reading
/// a recorder nothing writes to compares two frames that were never drawn.
pub(crate) fn mounted_recorder() -> Rc<FullFrame> {
    FULL_FRAMES.with(|held| {
        held.borrow_mut()
            .pop()
            .expect("an application was built with a recorder")
    })
}

/// Returns `compositor` behind the driver `ZGUI_DRIVER` selects.
///
/// A run that reads frames back waits for every step's frames, so each readback is the frame the
/// step drew. Any other run lets a driver with a thread of its own build while the window goes on.
pub(crate) fn client(
    compositor: Box<dyn compositor_api::Compositor>,
) -> Box<dyn compositor_api::CompositorClient> {
    if capturing() || WHOLE.with(Cell::get) {
        zgui_testkit_compositor::driver(compositor)
    } else {
        zgui_bench::renderers::client(compositor)
    }
}

/// Reports whether this run draws on the device and reads frames back, with `ZGUI_BENCH_CAPTURE`.
fn capturing() -> bool {
    std::env::var_os("ZGUI_BENCH_CAPTURE").is_some()
}

/// Builds what makes the renderer of each window an application opens.
///
/// The window's frame recorder is published for [`mounted_recorder`]. It records only when
/// [`draw_whole`] was on while this was built.
pub(crate) fn renderer() -> RendererFactory {
    let shown = Shown::default();
    let whole = WHOLE.with(Cell::get);
    FULL_FRAMES.with(|held| {
        held.borrow_mut()
            .push(Rc::new(FullFrame::new(shown.clone())));
    });
    Box::new(
        move |_surface: &Arc<dyn Surface>,
              target: RenderTarget|
              -> Result<compositor_api::RendererFactory, AppError> {
            let capture = capturing().then(|| CAPTURE.with(Clone::clone));
            let shown = shown.clone();
            let gpu = std::env::var_os("ZGUI_BENCH_GPU").is_some();
            Ok(Box::new(move || {
                if let Some(capture) = capture {
                    return Ok(
                        Box::new(crate::verify::Recorded::new(device(target)?, capture))
                            as Box<dyn Renderer>,
                    );
                }
                if whole {
                    let mut screen = Screen::new().showing_into(shown);
                    screen.configure(target);
                    return Ok(Box::new(screen));
                }
                if !gpu {
                    return Ok(Box::new(crate::phase::Planned::new(NullRenderer::new(
                        target,
                    ))));
                }
                let gpu = device(target)?;
                Ok(if crate::gpu_time::enabled() {
                    Box::new(crate::gpu_time::Timed::new(gpu))
                } else {
                    Box::new(gpu)
                })
            }))
        },
    )
}

/// Opens this machine's device offscreen for `target`, timing its passes when the run asks.
///
/// # Errors
///
/// Fails when no adapter opens a device.
fn device(target: RenderTarget) -> Result<WgpuRenderer, compositor_types::GpuUnavailable> {
    // The device reads whether to time its passes when it opens.
    if crate::gpu_time::enabled() {
        compositor_wgpu::enable_gpu_timing();
    }
    zgui_bench::renderers::device(target)
}
