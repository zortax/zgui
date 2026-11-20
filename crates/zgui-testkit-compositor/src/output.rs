//! One output of a compositor from the factory, with a renderer that draws it.

use compositor_api::{CommitReport, Compositor, OutputConfig, Renderer};
use compositor_capture::CaptureRenderer;
use compositor_types::{
    BufferAge, Built, ClientId, Counters, Frame, FrameClock, FrameRequest, OutputId, RenderTarget,
    Transaction,
};

use crate::{Unavailable, compositor, renderer};

/// A compositor, a renderer and one output that one client draws into.
///
/// Built from [`compositor`] and [`renderer`], so it runs against whichever implementations the
/// environment selects.
pub struct TestOutput {
    compositor: Box<dyn Compositor>,
    renderer: Box<dyn Renderer>,
    client: ClientId,
    output: OutputId,
    frame: Frame,
}

impl TestOutput {
    /// Creates an opaque output of `size` device pixels at scale 1, with a registered client.
    ///
    /// # Errors
    ///
    /// Fails when the factory cannot provide the compositor or the renderer.
    pub fn new(size: [u32; 2]) -> Result<Self, Unavailable> {
        let mut compositor = compositor()?;
        let mut renderer = renderer(size)?;
        let config = OutputConfig {
            size,
            scale: 1.0,
            opaque: true,
        };
        renderer.configure(RenderTarget {
            size: config.size,
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
            frame: Frame::default(),
        })
    }

    /// Returns the output the client draws into.
    pub fn output(&self) -> OutputId {
        self.output
    }

    /// Starts a transaction for the client.
    pub fn transaction(&mut self) -> Transaction {
        self.compositor.transaction(self.client)
    }

    /// Commits `transaction`.
    ///
    /// # Panics
    ///
    /// Panics when the compositor rejects the transaction.
    pub fn commit(&mut self, transaction: Transaction) -> CommitReport {
        match self.compositor.commit(transaction) {
            Ok(report) => report,
            Err(error) => panic!("the compositor rejected the transaction: {error:?}"),
        }
    }

    /// Builds the output's next frame and draws it, if there is one to draw.
    pub fn draw(&mut self) -> Built {
        let caps = self.renderer.capabilities();
        let request = FrameRequest {
            caps: &caps,
            age: self.renderer.target_age(),
            clock: FrameClock::default(),
            subpixel: None,
        };
        let built = self
            .compositor
            .build_frame(self.output, &request, &mut self.frame);
        if built == Built::Frame {
            let outcome = self.renderer.draw(&self.frame);
            self.compositor
                .frame_done(self.output, self.frame.id, &outcome);
        }
        built
    }

    /// Returns the compositor's counters for the last commit and the last frame.
    pub fn counters(&self) -> Counters {
        *self.compositor.counters()
    }

    /// Reports whether an unchanged output builds no frame: the compositor bounds damage and the
    /// renderer keeps its target.
    pub fn skips_idle_frames(&self) -> bool {
        self.compositor.capabilities().precise && self.renderer.target_age() == BufferAge::Kept
    }

    /// Returns what the last frame drew, as a capture transcript.
    ///
    /// `None` when the renderer is not the capture renderer or has drawn no frame.
    pub fn drawing(&self) -> Option<String> {
        let capture = self.renderer.as_any()?.downcast_ref::<CaptureRenderer>()?;
        Some(capture.last()?.drawing.as_str().to_owned())
    }
}
