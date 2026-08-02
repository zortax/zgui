//! The same document drawn at several device pixel ratios, read back off the graphics device.
//!
//! Every stage a window runs is run here: the events are dispatched, the reactive work flushed,
//! the document restyled, laid out, painted into a display list and drawn by the real
//! `zgui-render-wgpu` renderer. The one difference from a window on a screen is that the renderer
//! presents to a texture rather than to a compositor, which is what makes the pixels readable.
//!
//! ```text
//! cargo run --release -p zgui --example scale_capture -- /tmp/out 1.0 1.2 1.25 1.5 2.0
//! cargo run --release -p zgui --example scale_capture -- /tmp/out --mid 1.0 2.0 1.0
//! ```
//!
//! `--mid` keeps one window open across the whole list instead of opening a fresh one for each
//! ratio, which is how a window dragged between two monitors changes it. Running both and
//! comparing them is the whole point: a window that reaches a ratio and a window that opens at it
//! must draw the same picture, and only the first can hold an answer computed at another ratio.
//!
//! Each capture is written as `<tag>-scale-<ratio>.rgba`, four bytes a pixel, red first, beside a
//! `.txt` naming the extent. One JSON object a line goes to standard output.
//!
//! Two environment variables help when a figure needs explaining rather than reporting.
//! `ZGUI_DUMP` names a directory every box's resolved geometry is written into, one line each, for
//! diffing two runs. `ZGUI_NUDGE` resizes the window and back at the ratio it is already at before
//! each capture — the control for a difference blamed on a ratio change that an ordinary second
//! layout pass produces anyway.

use std::sync::{Arc, Mutex};

use zgui::prelude::*;
use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Rect, Scale, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_render::{FrameOutcome, RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, Pixels, wgpu};

#[path = "support/gallery.rs"]
mod gallery;

/// Where the last drawn frame's pixels are left for the caller to read.
type Readback = Arc<Mutex<Option<Pixels>>>;

/// A real renderer that also keeps a copy of every frame it drew.
///
/// The copy is taken inside `draw`, so what it holds is the frame that was presented rather than a
/// second render of the same scene.
struct Recording {
    /// The renderer under test.
    inner: zgui_render_wgpu::WgpuRenderer,
    /// Where the last frame's pixels go.
    into: Readback,
}

impl Renderer for Recording {
    fn capabilities(&self) -> zgui_render::RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &zgui_scene::Scene, damage: &DamageSet) -> FrameOutcome {
        let outcome = self.inner.draw(scene, damage);
        if let Some(pixels) = self.inner.read_presented() {
            *self.into.lock().expect("the readback is not poisoned") = Some(pixels);
        }
        outcome
    }

    fn register_external(
        &mut self,
        texture: zgui_render::ExternalTexture,
    ) -> zgui_render::TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: zgui_render::TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> zgui_render::MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        self.inner.texture_sink()
    }
}

/// What one capture says about itself.
struct Reading {
    /// The surface the frame was drawn into, in device pixels.
    surface: Size<DevicePx, Device>,
    /// The root box's border box, in device pixels.
    root: Rect<DevicePx, Device>,
    /// The badge's border box, in device pixels — one small element with a known CSS extent.
    badge: Option<Rect<DevicePx, Device>>,
    /// The first tile's border box, in device pixels.
    tile: Option<Rect<DevicePx, Device>>,
    /// How many pixels of the surface nothing ever drew.
    undrawn: u64,
    /// How many pixels the surface has.
    total: u64,
    /// The pixel in the very corner of the surface.
    corner: [u8; 4],
    /// How many distinct luminances a horizontal cut through the display type crosses.
    type_levels: usize,
    /// Two-by-two blocks in the display type's band that hold four identical pixels, and blocks.
    type_flat: (u64, u64),
    /// How many glyphs the frame had to rasterise, and how many it placed.
    glyphs: (u64, u64),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "/tmp/zgui-scale".to_owned());
    let rest: Vec<String> = args.collect();
    let mid = rest.iter().any(|arg| arg == "--mid");
    let mut scales: Vec<f64> = rest
        .iter()
        .filter(|arg| !arg.starts_with("--"))
        .map(|arg| arg.parse().expect("a scale is a number"))
        .collect();
    if scales.is_empty() {
        scales = vec![1.0, 1.2, 1.25, 1.5, 2.0];
    }
    std::fs::create_dir_all(&out)?;

    if mid {
        let readback: Readback = Arc::new(Mutex::new(None));
        let mut harness = open(Arc::clone(&readback))?;
        for scale in &scales {
            let reading = capture(&mut harness, &readback, *scale, &out, "mid")?;
            report(*scale, &reading, "mid");
        }
        harness.shut_down();
    } else {
        for scale in &scales {
            let readback: Readback = Arc::new(Mutex::new(None));
            let mut harness = open(Arc::clone(&readback))?;
            let reading = capture(&mut harness, &readback, *scale, &out, "fresh")?;
            report(*scale, &reading, "fresh");
            harness.shut_down();
        }
    }
    Ok(())
}

/// The gallery, on the headless platform, drawing through a real device into a texture.
fn open(
    into: Readback,
) -> Result<zgui_platform_headless::Harness<zgui_runtime::Runtime>, Box<dyn std::error::Error>> {
    let fonts = zgui::app::Fonts::system();
    let shaping = fonts.clone();
    let metrics = fonts.clone();
    let raster = fonts.clone();
    let runtime = zgui_runtime::App::new()
        .with_title("scale-capture")
        .with_size(gallery::WIDTH, gallery::HEIGHT)
        .with_stylesheet(gallery::SHEET)
        .with_renderer(Box::new(move |surface: &Arc<dyn Surface>, _target| {
            let size = surface.size();
            let target = RenderTarget::new(
                Size::new(size.width.0 as i32, size.height.0 as i32),
                Scale::new(surface.scale_factor() as f32),
            );
            let inner = Builder::new()
                .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
                .map_err(zgui_runtime::AppError::GpuUnavailable)?;
            Ok(Box::new(Recording {
                inner,
                into: Arc::clone(&into),
            }) as Box<dyn Renderer>)
        }))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(
            move |cx: &mut zgui_view::BuildCx<'_>| -> Box<dyn zgui_view::Anchor> {
                Box::new(gallery::view().into_view().build(cx))
            },
        )?;
    Ok(zgui_platform_headless::Harness::new(runtime))
}

/// Moves the window to `scale`, settles, and reads the frame back.
fn capture(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    readback: &Readback,
    scale: f64,
    out: &str,
    tag: &str,
) -> Result<Reading, Box<dyn std::error::Error>> {
    let size = Size::new(
        DevicePx((gallery::WIDTH * scale as f32).round()),
        DevicePx((gallery::HEIGHT * scale as f32).round()),
    );
    let before = glyph_counts();
    if std::env::var("ZGUI_NUDGE").is_ok() {
        // A resize at the same ratio, and back: the control for a difference that a scale change
        // is blamed for and an ordinary second layout pass would produce anyway.
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(size.width.0 - 40.0),
            DevicePx(size.height.0 - 40.0),
        )));
        harness.settle(64);
    }
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: scale,
        size,
    });
    harness.settle(64);
    let after = glyph_counts();

    let pixels = readback
        .lock()
        .expect("the readback is not poisoned")
        .take()
        .ok_or("no frame was drawn")?;
    let window = &harness.app().windows()[0];
    let reading = read(
        window,
        &pixels,
        size,
        (after.0 - before.0, after.1 - before.1),
    );
    if let Ok(dump) = std::env::var("ZGUI_DUMP") {
        dump_boxes(window, &format!("{dump}/{tag}-{scale}.txt"))?;
    }

    let name = format!("{out}/{tag}-scale-{scale}");
    std::fs::write(format!("{name}.rgba"), rgba(&pixels, size))?;
    std::fs::write(
        format!("{name}.txt"),
        format!("{} {}\n", size.width.0 as i32, size.height.0 as i32),
    )?;
    Ok(reading)
}

/// How many glyphs have been rasterised and how many placed, so far in this process.
fn glyph_counts() -> (u64, u64) {
    (
        zgui_profile::counter::get(zgui_profile::Counter::GlyphsRasterised),
        zgui_profile::counter::get(zgui_profile::Counter::GlyphsPlaced),
    )
}

/// The frame's bytes, red first, four to a pixel.
fn rgba(pixels: &Pixels, size: Size<DevicePx, Device>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((size.width.0 * size.height.0) as usize * 4);
    for y in 0..size.height.0 as i32 {
        for x in 0..size.width.0 as i32 {
            bytes.extend_from_slice(&pixels.rgba(x, y));
        }
    }
    bytes
}

/// Everything one capture is asked about.
fn read(
    window: &zgui_runtime::Window,
    pixels: &Pixels,
    size: Size<DevicePx, Device>,
    glyphs: (u64, u64),
) -> Reading {
    let layout = window.layout().borrow();
    let root_key = layout.root().expect("the document has a root box");
    let root = border_box(&layout, root_key).expect("the root box produced a fragment");
    let badge = by_class(window, &layout, "badge");
    let tile = by_class(window, &layout, "tile");

    let (width, height) = (size.width.0 as i32, size.height.0 as i32);
    let mut undrawn = 0_u64;
    for y in 0..height {
        for x in 0..width {
            let [r, g, b, _] = pixels.rgba(x, y);
            // The root's own background is #0b0d12, the darkest thing the document draws. Anything
            // darker in every channel is a pixel the frame never wrote: the composed target is
            // allocated zeroed and grow-only, and the blit copies it to the surface verbatim.
            if r < 6 && g < 6 && b < 6 {
                undrawn += 1;
            }
        }
    }

    let (levels, flat) = match by_class(window, &layout, "type__display") {
        Some(band) => sharpness(pixels, band, size),
        None => (0, (0, 0)),
    };

    Reading {
        surface: size,
        root,
        badge,
        tile,
        undrawn,
        total: (width as u64) * (height as u64),
        corner: pixels.rgba(width - 1, height - 1),
        type_levels: levels,
        type_flat: flat,
        glyphs,
    }
}

/// Every box's border box and unrounded layout, one line each, for comparing two passes.
fn dump_boxes(window: &zgui_runtime::Window, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let layout = window.layout().borrow();
    let document = window.document().borrow();
    let mut lines = Vec::new();
    for key in layout.keys() {
        let name = layout
            .get(key)
            .and_then(|node| node.source)
            .and_then(|source| document.store().index_of(source))
            .map(|index| {
                document
                    .store()
                    .classes_of(index)
                    .iter()
                    .map(|class| class.to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .unwrap_or_default();
        let rect = layout.layout_of(key);
        lines.push(format!("{key:?} {name} {rect:?}"));
    }
    lines.sort();
    std::fs::write(path, lines.join("\n"))?;
    Ok(())
}

/// The border box of the first fragment a box produced.
fn border_box(
    layout: &zgui_layout::LayoutStore,
    key: zgui_layout::BoxKey,
) -> Option<Rect<DevicePx, Device>> {
    let fragment = *layout.fragments_of_box(key).first()?;
    Some(layout.fragment(fragment)?.border_box)
}

/// The border box of the first element carrying `class`.
fn by_class(
    window: &zgui_runtime::Window,
    layout: &zgui_layout::LayoutStore,
    class: &str,
) -> Option<Rect<DevicePx, Device>> {
    let document = window.document().borrow();
    for key in layout.keys() {
        let Some(source) = layout.get(key).and_then(|node| node.source) else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if document
            .store()
            .classes_of(index)
            .iter()
            .any(|name| name.as_ref() == class)
        {
            return border_box(layout, key);
        }
    }
    None
}

/// How many luminances a cut through `band` crosses, and how flat its two-by-two blocks are.
///
/// A raster made at one ratio and stretched to another holds each of its own pixels several times
/// over: every two-by-two block of a doubled one is constant, and a cut across it crosses only the
/// levels the smaller raster had. A raster made at the ratio it is drawn at does neither.
fn sharpness(
    pixels: &Pixels,
    band: Rect<DevicePx, Device>,
    size: Size<DevicePx, Device>,
) -> (usize, (u64, u64)) {
    let clamp = |value: f32, high: f32| value.max(0.0).min(high - 1.0) as i32;
    let x0 = clamp(band.origin.x.0, size.width.0);
    let x1 = clamp(band.origin.x.0 + band.size.width.0, size.width.0);
    let y0 = clamp(band.origin.y.0, size.height.0);
    let y1 = clamp(band.origin.y.0 + band.size.height.0, size.height.0);

    let mut levels = std::collections::BTreeSet::new();
    let cut = y0 + (y1 - y0) * 2 / 5;
    for x in x0..x1 {
        let [r, g, b, _] = pixels.rgba(x, cut);
        levels.insert((u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000);
    }

    let (mut flat, mut blocks) = (0_u64, 0_u64);
    let mut y = y0;
    while y + 1 < y1 {
        let mut x = x0;
        while x + 1 < x1 {
            let corners = [
                pixels.rgba(x, y),
                pixels.rgba(x + 1, y),
                pixels.rgba(x, y + 1),
                pixels.rgba(x + 1, y + 1),
            ];
            blocks += 1;
            if corners.iter().all(|pixel| *pixel == corners[0]) {
                flat += 1;
            }
            x += 2;
        }
        y += 2;
    }
    (levels.len(), (flat, blocks))
}

/// One JSON object a line, which is what the caller reads.
fn report(scale: f64, reading: &Reading, tag: &str) {
    let rect = |rect: Option<Rect<DevicePx, Device>>| match rect {
        Some(rect) => format!(
            "[{},{},{},{}]",
            rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
        ),
        None => "null".to_owned(),
    };
    println!(
        "{{\"tag\":\"{tag}\",\"scale\":{scale},\
         \"surface\":[{},{}],\"root\":{},\"badge\":{},\"tile\":{},\
         \"undrawn\":{},\"total\":{},\"corner\":{:?},\
         \"type_levels\":{},\"type_flat\":[{},{}],\"glyphs\":[{},{}]}}",
        reading.surface.width.0,
        reading.surface.height.0,
        rect(Some(reading.root)),
        rect(reading.badge),
        rect(reading.tile),
        reading.undrawn,
        reading.total,
        reading.corner,
        reading.type_levels,
        reading.type_flat.0,
        reading.type_flat.1,
        reading.glyphs.0,
        reading.glyphs.1,
    );
}
