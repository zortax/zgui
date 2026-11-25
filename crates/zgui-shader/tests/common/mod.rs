//! A window that draws through a compositor onto a screen that runs application shaders.

// Every test binary compiles this module and uses a part of it.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use iceblink::{
    BufferAge, Frame, FrameOutcome, MemoryReport, ParamsBlock, RenderCapabilities, RenderTarget,
    ShaderClock,
};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_testkit_compositor::{Drawing, Instance, Screen};
use zgui_view::Anchor;
use zgui_view::BuildCx;

/// What the screens were given: the last picture, the parameter table and the shader clocks.
#[derive(Debug, Default)]
struct Record {
    drawing: Option<Drawing>,
    params: Vec<ParamsBlock>,
    params_written: usize,
    clocks: Vec<ShaderClock>,
}

/// What the [`Shading`] screens of a window show, readable after they were handed to it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seen {
    record: Arc<Mutex<Record>>,
}

impl Seen {
    /// Returns what the output shows.
    ///
    /// # Panics
    ///
    /// Panics when no frame was drawn.
    pub(crate) fn drawing(&self) -> Drawing {
        self.lock()
            .drawing
            .clone()
            .expect("the window drew a frame through its compositor")
    }

    /// Returns the shader quads the output shows.
    pub(crate) fn shader_quads(&self) -> Vec<Instance> {
        self.drawing().of_kind("shader_quad").cloned().collect()
    }

    /// Returns the quads the output shows.
    pub(crate) fn quads(&self) -> Vec<Instance> {
        self.drawing().quads().into_iter().cloned().collect()
    }

    /// Returns the parameter block the frame slot written in the field `params` of `item` holds.
    ///
    /// # Panics
    ///
    /// Panics when `item` names no slot the screen was given.
    pub(crate) fn params_of(&self, item: &Instance) -> ParamsBlock {
        let slot: usize = item
            .field("params")
            .and_then(|slot| slot.strip_prefix('#'))
            .and_then(|slot| slot.parse().ok())
            .unwrap_or_else(|| panic!("no parameter slot in `{}`", item.text()));
        self.lock().params[slot]
    }

    /// Returns how many parameter slots the screens were told to write, in every frame drawn.
    pub(crate) fn params_written(&self) -> usize {
        self.lock().params_written
    }

    /// Returns the shader clock of every frame drawn, in order.
    pub(crate) fn clocks(&self) -> Vec<ShaderClock> {
        self.lock().clocks.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Record> {
        self.record.lock().unwrap_or_else(|held| held.into_inner())
    }
}

/// A screen that reports it runs application shaders and keeps what [`Seen`] reads.
#[derive(Debug)]
pub(crate) struct Shading {
    screen: Screen,
    record: Arc<Mutex<Record>>,
}

impl Shading {
    /// Returns a screen that has drawn nothing and keeps what `seen` reads.
    pub(crate) fn new(seen: &Seen) -> Self {
        Self {
            screen: Screen::new(),
            record: Arc::clone(&seen.record),
        }
    }
}

impl iceblink::Renderer for Shading {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities {
            custom_shaders: true,
            ..self.screen.capabilities()
        }
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
        {
            let mut record = self.record.lock().unwrap_or_else(|held| held.into_inner());
            let delta = &frame.tables.params;
            if delta.replace {
                record.params = delta.all.clone();
            }
            record.params_written += delta.changed.len();
            for (slot, block) in &delta.changed {
                let slot = *slot as usize;
                if record.params.len() <= slot {
                    record.params.resize(slot + 1, *block);
                }
                record.params[slot] = *block;
            }
            record.clocks.push(frame.shader_clock);
        }
        let outcome = self.screen.draw(frame);
        self.record
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .drawing = self.screen.drawing();
        outcome
    }

    fn memory(&self) -> MemoryReport {
        self.screen.memory()
    }
}

/// Returns an application of one window that draws `view` styled by `css` onto a [`Shading`]
/// screen, and the handle that reads the screen.
pub(crate) fn app<V>(css: &str, view: V) -> (Harness<Runtime>, Seen)
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let seen = Seen::default();
    let screens = seen.clone();
    let handler = App::new()
        .with_title("shader")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_compositor(zgui_testkit_compositor::driver(
            zgui_testkit_compositor::compositor().expect("the test compositor exists"),
        ))
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = Shading::new(&screens);
                iceblink::Renderer::configure(&mut renderer, target);
                Ok(zgui_testkit_compositor::factory(move || renderer))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
        }))
        .with_custom(Box::new(|_document| zgui_custom::sources()))
        .into_handler(view)
        .expect("the reactive runtime installs");
    (Harness::new(handler), seen)
}
