//! A window that pushes tens of thousands of distinct glyphs through the compositor's atlas.
//!
//! The fixed face gives one glyph per character with the character's own scalar value as its
//! index, so a paragraph of characters nothing has drawn before is a paragraph of glyphs the
//! compositor has never rasterised, and 50 000 of them is 50 000 atlas entries.
//!
//! The window draws onto this machine's device, because the atlas pages the device holds are what
//! the budget is about.

use std::sync::Arc;

use compositor_api::{CompositorBudgets, Renderer};
use compositor_types::RenderTarget;
use zgui_platform::Surface;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_reactive::reexport::LocalStorage;
use zgui_runtime::{App, AppError};
use zgui_view::{BuildCx, IntoView, View};

/// The page, and the paragraph that is replaced every turn.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   text { display: block }";

/// How many distinct characters each turn puts on the page.
pub const PER_TURN: u32 = 250;

/// Where the characters are drawn from.
///
/// The supplementary ideographic plane: no surrogate halves inside it, so every step is a valid
/// scalar, and low enough in it that fifty thousand steps stay inside. The offset keeps the low
/// sixteen bits, which the fixed face uses as a glyph index, clear of the space character.
const FIRST_SCALAR: u32 = 0x2_0100;

/// A window holding the fixture, and the signal that drives it.
pub struct Churn {
    /// The window and the loop around it.
    pub harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The paragraph that is replaced every turn.
    text: RwSignal<String, LocalStorage>,
    /// How many distinct characters have been shown so far.
    shown: u32,
}

impl Churn {
    /// Opens the window on this machine's device with its compositor's atlas held to
    /// `atlas_bytes`, and settles it, or returns `None` when no device opens.
    ///
    /// `None` as the limit is a compositor with no budget at all, which is the control every
    /// non-vacuity assertion needs.
    pub fn open(atlas_bytes: Option<u64>) -> Option<Self> {
        if let Err(why) = zgui_testkit_compositor::Device::open([64, 64]) {
            eprintln!("skipped: {why}");
            return None;
        }
        let text = RwSignal::new_local(String::from("start"));
        let mut compositor = crate::support::client();
        compositor.set_budgets(CompositorBudgets { atlas_bytes });
        let handler = App::new()
            .with_title("churn")
            .with_size(400.0, 300.0)
            .with_stylesheet(CSS)
            .with_compositor(compositor)
            .with_renderer(Box::new(device))
            .with_text_engine(Box::new(|| {
                Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
            }))
            .into_handler(move |cx: &mut BuildCx<'_>| {
                Box::new(
                    zgui_elements::column()
                        .child(zgui_elements::text().child(move || text.get()))
                        .into_view()
                        .build(cx),
                )
            })
            .expect("the reactive runtime installs");
        let mut harness = zgui_platform_headless::Harness::new(handler);
        harness.settle(16);
        Some(Self {
            harness,
            text,
            shown: 0,
        })
    }

    /// Runs one turn: a fresh paragraph of characters nothing has drawn before.
    pub fn turn(&mut self) {
        let mut fresh = String::with_capacity(PER_TURN as usize * 4);
        for _ in 0..PER_TURN {
            let scalar = FIRST_SCALAR + self.shown;
            self.shown += 1;
            fresh.push(char::from_u32(scalar).expect("the plane holds no surrogates"));
        }
        self.text.set(fresh);
        self.harness.settle(4);
    }

    /// How many distinct characters have been drawn since the window opened.
    pub fn shown(&self) -> u32 {
        self.shown
    }

    /// The bytes of atlas pages the window's renderer holds.
    pub fn atlas_bytes(&self) -> u64 {
        self.harness.app().windows()[0]
            .output_status()
            .map_or(0, |status| status.memory.atlas)
    }
}

/// Opens the device a churning window draws through.
fn device(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<compositor_api::RendererFactory, AppError> {
    Ok(Box::new(move || {
        let mut device = zgui_testkit_compositor::Device::open(target.size)
            .expect("a device opens, which `Churn::open` answered beforehand");
        device.configure(target);
        Ok(Box::new(device) as Box<dyn Renderer>)
    }))
}

/// What the counters did between `before` and now.
pub fn moved(before: &zgui_profile::Counters) -> zgui_profile::Counters {
    before.delta(&zgui_profile::counter::snapshot())
}
