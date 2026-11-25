//! The screens a test's windows draw onto, readable after their renderers went to the driver.

use std::cell::RefCell;

use iceblink::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    RendererFactory,
};

use crate::screen::{Screen, Shown};
use crate::select::RendererKind;

thread_local! {
    /// What each screen made on this thread shows, in the order they were made.
    static MADE: RefCell<Vec<Shown>> = const { RefCell::new(Vec::new()) };
}

/// Returns a factory of a [`Screen`] configured for `target`, whose frames [`screens`] reads,
/// drawing through the renderer [`through`] selects.
pub fn screen(target: RenderTarget) -> RendererFactory {
    let shown = Shown::default();
    MADE.with(|made| made.borrow_mut().push(shown.clone()));
    Box::new(move || Ok(showing(shown, target)))
}

/// Returns a [`Screen`] configured for `target` that shows its frames through `shown`, and draws
/// them through the renderer [`through`] selects as well.
pub fn showing(shown: Shown, target: RenderTarget) -> Box<dyn Renderer> {
    let screen = Screen::new().showing_into(shown);
    let mut renderer: Box<dyn Renderer> = match through(target) {
        Some(inner) => Box::new(Through { screen, inner }),
        None => Box::new(screen),
    };
    renderer.configure(target);
    renderer
}

/// Returns the renderer `ZGUI_RENDERER` selects for a screen of `target` to draw through as well,
/// or `None` for the recording renderer itself.
///
/// # Panics
///
/// Panics when the variable holds an unknown value or names a device that does not open.
pub fn through(target: RenderTarget) -> Option<Box<dyn Renderer>> {
    let kind = RendererKind::from_environment().unwrap_or_else(|why| panic!("no renderer: {why}"));
    (kind != RendererKind::Capture)
        .then(|| crate::renderer(target.size).unwrap_or_else(|why| panic!("no renderer: {why}")))
}

/// Returns what each screen made on this thread by [`screen`] shows, in the order they were
/// made.
pub fn screens() -> Vec<Shown> {
    MADE.with(|made| made.borrow().clone())
}

/// A [`Screen`] whose frames are drawn through another renderer as well, which answers for them.
struct Through {
    screen: Screen,
    inner: Box<dyn Renderer>,
}

impl Renderer for Through {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.screen.configure(target);
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn target_age(&self) -> BufferAge {
        self.inner.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        self.screen.draw(frame);
        self.inner.draw(frame)
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }
}
