//! Chunks drawn by the compositor onto a graphics device, and read back as pixels.

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use iceblink::{
    BufferAge, Built, ChunkKey, ClientId, Compositor, Frame, FrameClock, FrameRequest, OrderEntry,
    OrderNode, OutputConfig, OutputId, RenderTarget, Renderer,
};
use zgui_paint::Tables;
use zgui_paint::emit::Recorder;
use zgui_space::{ClipId, SpatialId};
use zgui_testkit_compositor::{Composed, Device, Drawing, Pixels, Shown};

/// The key the picture is recorded under.
const KEY: ChunkKey = ChunkKey::new(1, 0);

/// Serialises the tests of one binary that draw on the device.
fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What one picture draws: its pixels and its transcript.
pub(crate) struct Picture {
    /// The composed target.
    pub(crate) pixels: Arc<Pixels>,
    /// The frame's transcript.
    pub(crate) drawing: Drawing,
}

/// A compositor drawing one chunk at a time onto a square device target.
///
/// An opaque target draws each chunk over black. A clear target draws it over nothing.
pub(crate) struct Gpu {
    compositor: Box<dyn Compositor>,
    device: Device,
    composed: Composed,
    shown: Shown,
    client: ClientId,
    output: OutputId,
    config: OutputConfig,
    frame: Frame,
    side: u32,
    opaque: bool,
    _guard: MutexGuard<'static, ()>,
}

impl Gpu {
    /// Opens an opaque target of `side` by `side` pixels, or `None`, saying why, when no device
    /// opens.
    pub(crate) fn open(side: u32) -> Option<Self> {
        Self::with(side, true)
    }

    /// Opens a clear target of `side` by `side` pixels, or `None` when no device opens.
    pub(crate) fn clear(side: u32) -> Option<Self> {
        Self::with(side, false)
    }

    fn with(side: u32, opaque: bool) -> Option<Self> {
        let guard = device_lock();
        let mut device = match Device::open([side, side]) {
            Ok(device) => device,
            Err(why) => {
                eprintln!("skipped: {why}");
                return None;
            }
        };
        let mut compositor = zgui_testkit_compositor::compositor()
            .unwrap_or_else(|why| panic!("no compositor: {why}"));
        let config = OutputConfig {
            size: [side, side],
            scale: 1.0,
            opaque,
        };
        device.configure(RenderTarget {
            size: config.size,
            scale: config.scale,
            opaque: config.opaque,
        });
        let (client, _ids) = compositor.register_client();
        let output = compositor.create_output(config);
        let mut gpu = Self {
            composed: device.composed(),
            shown: device.shown(),
            compositor,
            device,
            client,
            output,
            config,
            frame: Frame::default(),
            side,
            opaque,
            _guard: guard,
        };
        let mut transaction = gpu.compositor.transaction(gpu.client);
        let root = transaction.create_order(OrderNode::default());
        transaction.set_root(gpu.output, Some(root));
        transaction.splice(root, 0, 0, &[OrderEntry::Chunk(KEY)]);
        gpu.commit(transaction);
        Some(gpu)
    }

    /// Returns the side of the target.
    pub(crate) fn side(&self) -> u32 {
        self.side
    }

    /// Records what `emit` records as the one chunk of the output, draws the whole output, and
    /// returns what it drew.
    pub(crate) fn picture(
        &mut self,
        scene: &mut Tables,
        space: SpatialId,
        emit: impl FnOnce(&mut Recorder<'_>),
    ) -> Picture {
        let side = self.side as f32;
        let opaque = self.opaque;
        let mut transaction = self.compositor.transaction(self.client);
        zgui_paint::record_chunk(&mut transaction, KEY, scene, (space, ClipId::ROOT), |rec| {
            if !opaque {
                emit(rec);
                return;
            }
            let black = rec.color(zgui_color::Color::BLACK);
            let mut ground = zgui_paint::emit::record::filled(
                zgui_geom::Rect::new(
                    zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
                    zgui_geom::Size::new(zgui_geom::DevicePx(side), zgui_geom::DevicePx(side)),
                ),
                [0.0; 8],
                black,
            );
            ground.space = rec.space(SpatialId::VIEWPORT);
            rec.quad(ground);
            emit(rec);
        });
        self.commit(transaction);
        self.redraw()
    }

    /// Draws the whole output again and returns what it drew.
    pub(crate) fn redraw(&mut self) -> Picture {
        let translucent = OutputConfig {
            opaque: !self.config.opaque,
            ..self.config
        };
        self.compositor.configure_output(self.output, translucent);
        self.compositor.configure_output(self.output, self.config);
        let caps = self.device.capabilities();
        let request = FrameRequest {
            caps: &caps,
            age: BufferAge::Unknown,
            clock: FrameClock::default(),
            subpixel: None,
        };
        let built = self
            .compositor
            .build_frame(self.output, &request, &mut self.frame);
        assert_eq!(built, Built::Frame, "a whole output always draws");
        let outcome = self.device.draw(&self.frame);
        self.compositor
            .frame_done(self.output, self.frame.id, &outcome);
        Picture {
            pixels: self.composed.pixels().expect("a frame was composed"),
            drawing: self.shown.drawing().expect("a frame was drawn"),
        }
    }

    fn commit(&mut self, transaction: iceblink::Transaction) {
        if let Err(error) = self.compositor.commit(transaction) {
            panic!("the compositor rejected the transaction: {error}");
        }
    }
}
