//! One client drawing into one output, with every frame kept as a transcript.

use std::sync::Arc;

use compositor_api::{Compositor, OutputConfig, Renderer};
use compositor_capture::CaptureRenderer;
use compositor_testing::fakes::{FakeFontRegistry, FakeGlyphRaster, fake_font};
use compositor_types::{
    BufferAge, Built, ChunkKey, ClientId, CommitCounters, Counters, Frame, FrameClock,
    FrameRequest, OrderEntry, OrderId, OrderNode, OutputId, RenderTarget, Transaction,
};

use crate::Unavailable;
use crate::drawing::Drawing;

/// How many fonts a session's registry knows, with ids from zero.
pub const FONTS: u32 = 16;

/// Gives `compositor` a font registry that knows the fonts `0..FONTS` and a glyph rasteriser that
/// draws every glyph as a box on the baseline, half the em size wide and three quarters of it
/// high.
pub fn with_fake_text(compositor: &mut dyn Compositor) {
    let fonts = FakeFontRegistry::new();
    for seed in 0..FONTS {
        compositor_api::FontRegistry::register(&fonts, fake_font(seed));
    }
    compositor.use_font_registry(Arc::new(fonts));
    compositor.use_glyph_raster(Arc::new(FakeGlyphRaster::new()));
}

/// A compositor with one client and one output, drawn by a recording renderer.
///
/// The compositor is the one `ZGUI_COMPOSITOR` selects, with the text of [`with_fake_text`].
pub struct Session {
    compositor: Box<dyn Compositor>,
    renderer: CaptureRenderer,
    client: ClientId,
    output: OutputId,
    config: OutputConfig,
    frame: Frame,
    shown: Option<(OrderId, u32)>,
}

impl Session {
    /// Creates a session whose output is `size` device pixels at scale one and opaque.
    ///
    /// # Errors
    ///
    /// Fails when the environment selects no compositor that exists.
    pub fn new(size: [u32; 2]) -> Result<Self, Unavailable> {
        let mut compositor = crate::compositor()?;
        with_fake_text(&mut *compositor);

        let config = OutputConfig {
            size,
            scale: 1.0,
            opaque: true,
        };
        let mut renderer = CaptureRenderer::new();
        renderer.configure(RenderTarget {
            size,
            scale: config.scale,
            opaque: config.opaque,
        });
        let (client, _ids) = compositor.register_client();
        let output = compositor.create_output(config);
        Ok(Self {
            compositor,
            renderer,
            client,
            output,
            config,
            frame: Frame::default(),
            shown: None,
        })
    }

    /// Returns the output the session draws.
    pub fn output(&self) -> OutputId {
        self.output
    }

    /// Returns an empty transaction of the session's client.
    pub fn transaction(&mut self) -> Transaction {
        self.compositor.transaction(self.client)
    }

    /// Drops everything the session's client holds and registers it again with nothing.
    pub fn restart_client(&mut self) {
        self.compositor.unregister_client(self.client);
        let (client, _ids) = self.compositor.register_client();
        self.client = client;
        self.shown = None;
    }

    /// Makes the output draw the chunks `keys` in that order, through `transaction`.
    ///
    /// The first call creates the output's root order node. A later call replaces the keys of
    /// the one before.
    pub fn show(&mut self, transaction: &mut Transaction, keys: &[ChunkKey]) {
        let entries: Vec<OrderEntry> = keys.iter().copied().map(OrderEntry::Chunk).collect();
        let (root, held) = match self.shown {
            Some(shown) => shown,
            None => {
                let root = transaction.create_order(OrderNode::default());
                transaction.set_root(self.output, Some(root));
                (root, 0)
            }
        };
        transaction.splice(root, 0, held, &entries);
        self.shown = Some((root, keys.len() as u32));
    }

    /// Commits `transaction` and returns what the commit did.
    ///
    /// # Panics
    ///
    /// Panics with the reason when the compositor rejects the transaction.
    pub fn commit(&mut self, transaction: Transaction) -> CommitCounters {
        match self.compositor.commit(transaction) {
            Ok(report) => report.counters,
            Err(error) => panic!("the compositor rejected the transaction: {error}"),
        }
    }

    /// Builds and draws the next frame, and returns what it draws.
    ///
    /// The renderer keeps its target, so the frame draws only the damage. Returns `None` when
    /// the compositor had nothing to draw.
    pub fn frame(&mut self) -> Option<Drawing> {
        self.draw(self.renderer.target_age())
    }

    /// Builds and draws a frame of the whole output, and returns what it draws.
    ///
    /// The output is configured as translucent and back, which damages the whole output.
    pub fn redraw(&mut self) -> Drawing {
        let config = self.config;
        let translucent = OutputConfig {
            opaque: !config.opaque,
            ..config
        };
        self.compositor.configure_output(self.output, translucent);
        self.compositor.configure_output(self.output, config);
        self.draw(BufferAge::Unknown)
            .expect("a frame of unknown age draws the whole output")
    }

    /// Returns the compositor's counters, which cover the last commit and the last frame.
    pub fn counters(&self) -> &Counters {
        self.compositor.counters()
    }

    /// Returns the compositor.
    pub fn compositor(&mut self) -> &mut dyn Compositor {
        &mut *self.compositor
    }

    fn draw(&mut self, age: BufferAge) -> Option<Drawing> {
        let caps = self.renderer.capabilities();
        let request = FrameRequest {
            caps: &caps,
            age,
            clock: FrameClock::default(),
            subpixel: None,
        };
        let built = self
            .compositor
            .build_frame(self.output, &request, &mut self.frame);
        if built != Built::Frame {
            return None;
        }
        let outcome = self.renderer.draw(&self.frame);
        self.compositor
            .frame_done(self.output, self.frame.id, &outcome);
        let captured = self.renderer.last().expect("a frame was just drawn");
        Some(Drawing::of(captured.drawing.clone()))
    }
}

impl core::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("client", &self.client)
            .field("output", &self.output)
            .finish_non_exhaustive()
    }
}
