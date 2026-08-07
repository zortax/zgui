//! An instrumented run of the counter: a real window, a real device, and what every frame did.
//!
//! It is the counter example with one thing added — the renderer it draws through is wrapped, so
//! each frame records when it happened, what it cost, which rectangles it promised to redraw and
//! how many device pixels those covered. Frames worth looking at are read back off the device and
//! written out as `.ppm`, which is what turns "it drew" into a file somebody can measure.
//!
//! Run it with `ZGUI_PROBE_DIR=/somewhere cargo run -p zgui --example probe`.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use zgui::prelude::*;
use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_platform::{PlatformError, Surface};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_render_wgpu::{Builder, WgpuRenderer};
use zgui_runtime::AppError;
use zgui_scene::Scene;

/// How many readbacks the run is allowed to write before it stops writing them.
const SHOT_LIMIT: u32 = 12;

/// A number, and two buttons that change it. The counter example's component, unchanged.
#[component]
fn Counter(
    /// Where the count starts.
    #[prop(default = 0)]
    start: i32,
) -> impl IntoView {
    let (count, set_count) = signal(start);

    view! {
        column(class = "counter", a11y:role = Role::Group, a11y:label = "Counter") {
            label(class = "counter__caption") {"Count"}
            text(class = "counter__value") {{move || count.get().to_string()}}
            row(class = "counter__buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Decrease",
                    on:click = move |_| set_count.update(|n| *n -= 1)
                ) {
                    "-"
                }
                control(
                    class = "button button--primary",
                    tabindex = Focus::Sequential,
                    a11y:label = "Increase",
                    on:click = move |_| set_count.update(|n| *n += 1)
                ) {
                    "+"
                }
            }
            label(class = "counter__parity") {
                {move || if count.get() % 2 == 0 { "even" } else { "odd" }}
            }
        }
    }
}

/// The counter example's style sheet, unchanged.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .counter {
        align-items: center;
        gap: 12px;
        padding: 32px 48px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
        box-shadow: 0 18px 40px rgba(0, 0, 0, 0.45);
    }

    .counter__caption {
        font-size: 13px;
        letter-spacing: 2px;
        color: #7d879b;
    }

    .counter__value {
        font-size: 64px;
        font-weight: 700;
        line-height: 1.1;
    }

    .counter__parity {
        font-size: 13px;
        color: #7d879b;
    }

    .counter__buttons { gap: 12px; }

    .button {
        padding: 10px 22px;
        border-radius: 10px;
        border: 1px solid #2f3646;
        background-color: #232936;
        color: #e8ecf4;
        font-size: 20px;
        line-height: 1;
        text-align: center;
    }

    .button:hover { background-color: #2b3243; }

    .button--primary {
        background-color: #3b6cf6;
        border-color: #3b6cf6;
    }

    .button--primary:hover { background-color: #4d7bff; }"
);

/// A renderer that draws through the machine's own device and writes down what each frame did.
struct Probe {
    /// What actually draws.
    inner: WgpuRenderer,
    /// When the process started, which is what a cold start is measured from.
    start: Instant,
    /// Where readbacks and the frame log go.
    dir: PathBuf,
    /// The frame log, one JSON object per line.
    log: BufWriter<File>,
    /// How many frames have been drawn.
    frames: u64,
    /// How many readbacks have been written.
    shots: u32,
}

impl Probe {
    /// Wraps `inner`, writing everything under `dir`.
    fn new(inner: WgpuRenderer, start: Instant, dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let log = BufWriter::new(
            File::create(dir.join("frames.jsonl")).expect("the probe's own log is writable"),
        );
        Self {
            inner,
            start,
            dir,
            log,
            frames: 0,
            shots: 0,
        }
    }

    /// Copies the composed target off the device and writes it out as a binary PPM.
    ///
    /// The composed target is what the copy to the surface reads, so this is the frame that was
    /// presented rather than a second rendering of the same scene.
    fn snapshot(&mut self, name: &str) -> Option<PathBuf> {
        let pixels = self.inner.read_composed();
        let size = pixels.size();
        let path = self.dir.join(name);
        let mut file = BufWriter::new(File::create(&path).ok()?);
        write!(file, "P6\n{} {}\n255\n", size.width, size.height).ok()?;
        let mut row = Vec::with_capacity((size.width * 3) as usize);
        for y in 0..size.height {
            row.clear();
            for x in 0..size.width {
                let [r, g, b, _] = pixels.rgba(x, y);
                row.extend_from_slice(&[r, g, b]);
            }
            file.write_all(&row).ok()?;
        }
        file.flush().ok()?;
        self.shots += 1;
        Some(path)
    }

    /// Writes one line of the frame log.
    fn record(
        &mut self,
        at: f64,
        cost: f64,
        damage: &DamageSet,
        outcome: FrameOutcome,
        shot: &str,
    ) {
        let rects: Vec<String> = damage
            .rects()
            .iter()
            .map(|r| {
                format!(
                    "[{},{},{},{}]",
                    r.origin.x, r.origin.y, r.size.width, r.size.height
                )
            })
            .collect();
        let (kind, calls, damage_px, uploaded) = match outcome {
            FrameOutcome::Presented(stats) => (
                "presented",
                stats.draw_calls,
                stats.damage_px,
                stats.bytes_uploaded,
            ),
            FrameOutcome::Skipped(reason) => {
                let _ = writeln!(
                    self.log,
                    r#"{{"frame":{},"at_ms":{at:.3},"outcome":"skipped","reason":"{reason:?}"}}"#,
                    self.frames
                );
                let _ = self.log.flush();
                return;
            }
            FrameOutcome::Recovered => ("recovered", 0, 0, 0),
            _ => ("unknown", 0, 0, 0),
        };
        let _ = writeln!(
            self.log,
            r#"{{"frame":{},"at_ms":{at:.3},"cost_ms":{cost:.3},"outcome":"{kind}","full_damage":{},"rects":[{}],"draw_calls":{calls},"damage_px":{damage_px},"bytes_uploaded":{uploaded},"shot":"{shot}"}}"#,
            self.frames,
            damage.is_full(),
            rects.join(",")
        );
        let _ = self.log.flush();
    }
}

impl Renderer for Probe {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let began = Instant::now();
        let outcome = self.inner.draw(scene, damage);
        let cost = began.elapsed().as_secs_f64() * 1e3;
        let at = self.start.elapsed().as_secs_f64() * 1e3;
        self.frames += 1;

        // The first frame is the cold one, and a partial frame is the interesting one: it is the
        // only kind that can show that a change redrew less than the window.
        let wanted = matches!(outcome, FrameOutcome::Presented(_))
            && self.shots < SHOT_LIMIT
            && (self.frames == 1 || !damage.is_full());
        let shot = if wanted {
            let name = format!("frame-{:03}.ppm", self.frames);
            self.snapshot(&name).map_or(String::new(), |_| name)
        } else {
            String::new()
        };
        self.record(at, cost, damage, outcome, &shot);
        outcome
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        // Forwarded so the wgpu companion crate reaches the real backend through this wrapper.
        self.inner.as_any_mut()
    }
}

/// Opens this machine's graphics device for `surface` and wraps it in the probe.
fn renderer(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
    start: Instant,
    dir: &Path,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        )));
    };
    let presented = Arc::clone(surface);
    let builder = Builder::new().with_pre_present(Box::new(move || {
        presented.pre_present_notify();
    }));
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    let inner = builder.for_surface(target, drawable)?;
    Ok(Box::new(Probe::new(inner, start, dir.to_path_buf())))
}

fn main() -> Result<(), zgui::Error> {
    let start = Instant::now();
    let dir = PathBuf::from(
        std::env::var("ZGUI_PROBE_DIR").unwrap_or_else(|_| "target/probe".to_owned()),
    );
    let factory = {
        let dir = dir.clone();
        Box::new(move |surface: &Arc<dyn Surface>, target: RenderTarget| {
            renderer(surface, target, start, &dir)
        })
    };
    // A style sheet from a file, so that "is this rule what inflated the damage?" is one restart
    // away from an answer rather than a rebuild.
    let sheet = std::env::var("ZGUI_PROBE_SHEET")
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_else(|| SHEET.to_owned());
    app()
        .with_title("Counter Probe")
        .with_size(360.0, 300.0)
        .with_stylesheet(sheet)
        .with_renderer(factory)
        .run(|| view! { Counter(start = 0) })
}
