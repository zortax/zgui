//! What a window draws through, and what records the display list it drew.

use std::rc::Rc;
use std::sync::Arc;

use compositor_api::Renderer;
use compositor_types::RenderTarget;
use compositor_wgpu::WgpuRenderer;
use zgui::platform::Surface;
use zgui::runtime::AppError;
use zgui_bench::renderers::NullRenderer;

thread_local! {
    /// Where the pixel comparison leaves the readback it asked for.
    pub(crate) static CAPTURE: Rc<crate::verify::Capture> = Rc::default();

    /// Where each window's display-list recorder is left for its opener to pick up, oldest first.
    ///
    /// A recorder is built with the application that opens the window, so this is the only place
    /// the two can be introduced. A slot rather than a list would hold only the window opened
    /// last, and the differential opens two.
    static FULL_FRAMES: std::cell::RefCell<Vec<Rc<crate::verify::FullFrame>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The display-list recorder of the application that was built most recently.
///
/// # Panics
///
/// Panics when no application has been built since the last call, because a comparison reading
/// a recorder nothing writes to compares two frames that were never drawn.
pub(crate) fn mounted_recorder() -> Rc<crate::verify::FullFrame> {
    FULL_FRAMES.with(|held| {
        held.borrow_mut()
            .pop()
            .expect("an application was built with a recorder")
    })
}

/// Builds the probe that records every frame's display list for the comparisons and phases.
///
/// Its recorder is published for [`mounted_recorder`]. Keeping the list costs nothing until
/// something asks for it: the recorder is off until a phase turns it on.
pub(crate) fn recorder() -> Rc<dyn zgui::runtime::FrameProbe> {
    let full = Rc::new(crate::verify::FullFrame::default());
    FULL_FRAMES.with(|held| held.borrow_mut().push(Rc::clone(&full)));
    Rc::new(crate::verify::Listed {
        full,
        capture: capturing().then(|| CAPTURE.with(Clone::clone)),
    })
}

/// Reports whether this run draws on the device and reads frames back, with `ZGUI_BENCH_CAPTURE`.
fn capturing() -> bool {
    std::env::var_os("ZGUI_BENCH_CAPTURE").is_some()
}

/// Builds the renderer a window draws through: nothing, or this machine's device offscreen.
pub(crate) fn renderer(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    if capturing() {
        return Ok(Box::new(crate::verify::Recorded {
            inner: device(target)?,
            capture: CAPTURE.with(Clone::clone),
        }));
    }
    if std::env::var_os("ZGUI_BENCH_GPU").is_none() {
        return Ok(Box::new(NullRenderer::new(target)));
    }
    let gpu = device(target)?;
    Ok(if crate::gpu_time::enabled() {
        Box::new(crate::gpu_time::Timed::new(gpu))
    } else {
        Box::new(gpu)
    })
}

/// Opens this machine's device offscreen for `target`, timing its passes when the run asks.
///
/// # Errors
///
/// Fails when no adapter opens a device.
fn device(target: RenderTarget) -> Result<WgpuRenderer, AppError> {
    // The device reads whether to time its passes when it opens.
    if crate::gpu_time::enabled() {
        compositor_wgpu::enable_gpu_timing();
    }
    zgui_bench::renderers::device(target)
}
