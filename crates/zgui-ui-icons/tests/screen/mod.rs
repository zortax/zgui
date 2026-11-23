//! A compositor renderer that keeps what each frame drew and draws nothing.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several test binaries, and none of them uses all of it"
)]

use compositor_types::{
    BufferAge, Frame, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget,
};
use zgui_testkit_compositor::{Drawing, Instance, Screen};

/// What a frame drew, handed over after every frame.
pub type Watch = Box<dyn FnMut(&Drawing) + Send>;

/// A recording renderer that hands each frame's transcript to a watcher.
struct Watching {
    screen: Screen,
    watch: Watch,
}

impl compositor_api::Renderer for Watching {
    fn capabilities(&self) -> RenderCapabilities {
        self.screen.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.screen.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.screen.target()
    }

    fn target_age(&self) -> BufferAge {
        self.screen.target_age()
    }

    fn draw(&mut self, frame: &Frame) -> FrameOutcome {
        let outcome = self.screen.draw(frame);
        if let Some(drawing) = self.screen.drawing() {
            (self.watch)(&drawing);
        }
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.screen.memory()
    }
}

/// Returns a renderer maker whose renderers hand every frame to the watcher `make` builds.
pub fn watched(mut make: impl FnMut() -> Watch + Send + 'static) -> zgui::CompositorRendererMaker {
    Box::new(move |_size: [u32; 2]| {
        Box::new(Watching {
            screen: Screen::new(),
            watch: make(),
        }) as Box<dyn compositor_api::Renderer>
    })
}

/// The width and height of every shape a frame drew: its paths, marks, path masks and quads.
pub fn shapes(drawing: &Drawing) -> Vec<(&Instance, [f64; 2])> {
    drawing
        .instances()
        .iter()
        .filter(|instance| matches!(instance.kind(), "path" | "marks" | "mono" | "quad"))
        .map(|instance| {
            let [_, _, width, height] = instance.rect("ink").unwrap_or_else(|| instance.bounds());
            (instance, [f64::from(width), f64::from(height)])
        })
        .collect()
}
