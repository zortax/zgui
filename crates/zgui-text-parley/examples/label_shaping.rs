//! The fixed cost of one short paragraph: thirty numeric labels, new strings every round.
//!
//! A plot that zooms relabels its axes on most frames, so every label is a cache miss and the
//! cost that decides the frame is what one shape and one break of a five-character string costs,
//! not how fast long text shapes. This drives the real engine path — key, shape, break — the way
//! layout drives it, and prints microseconds per label.
//!
//! ```text
//! cargo run -p zgui-text-parley --release --example label_shaping [system]
//! ```
//!
//! `system` resolves `sans-serif` against the installed faces, which is what an application does;
//! without it only the shipped face is registered.

use std::sync::Arc;
use std::time::Instant;

use zgui_geom::CssPx;
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, FontSource, ParagraphContent, ParagraphKey, ParagraphShaper, StyledRun, TextMap,
};
use zgui_text_parley::{FontSystem, FontSystemOptions, Shaper};
use zgui_text_style::{FamilyName, FontFamilyList, GenericFamily, ParagraphStyle, TextStyle};

/// Labels per round, as many as a plot's two axes carry.
const LABELS: usize = 30;

/// Rounds before anything is timed.
const WARMUP: usize = 300;

/// Rounds per timed sample.
const ROUNDS: usize = 300;

/// Timed samples.
const SAMPLES: usize = 9;

/// The label text for one round, different from every other round's.
fn label(round: usize, index: usize) -> String {
    let value = round as f64 * 0.0137 + index as f64 * 0.25 - 4.0;
    format!("{value:.2}")
}

fn main() {
    let system = std::env::args().any(|argument| argument == "system");
    let fonts = if system {
        Arc::new(FontSystem::new(FontSystemOptions::with_system_fonts()))
    } else {
        let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
        let path = format!(
            "{}/tests/fonts/NotoSans-Regular.ttf",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> =
            Arc::new(std::fs::read(&path).expect("the shipped face"));
        fonts.register(bytes, None).expect("a readable face");
        fonts
    };
    let mut shaper = Shaper::new(fonts);
    let style = Arc::new(TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Generic(GenericFamily::SansSerif)]),
        size: CssPx(11.0),
        ..TextStyle::initial()
    });
    let paragraph = ParagraphStyle::initial();

    let round = |round: usize, shaper: &mut Shaper| -> f32 {
        let mut width = 0.0;
        for index in 0..LABELS {
            let text = label(round, index);
            let mut map = TextMap::new();
            map.push(0..text.len(), 0, 0);
            let runs = [StyledRun {
                text: 0..text.len(),
                style: Arc::clone(&style),
                brush: PaintSlot(0),
            }];
            let content = ParagraphContent {
                text: &text,
                map: &map,
                runs: &runs,
                boxes: &[],
                paragraph: &paragraph,
                scale: 1.0,
            };
            let mut shaped = shaper.shape_keyed(ParagraphKey::of(&content), &content);
            let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, None));
            width += broken.geometry.size.width.0;
        }
        width
    };

    let mut sink = 0.0;
    for index in 0..WARMUP {
        sink += round(index, &mut shaper);
    }
    let mut samples: Vec<f64> = (0..SAMPLES)
        .map(|sample| {
            let started = Instant::now();
            for index in 0..ROUNDS {
                sink += round(WARMUP + sample * ROUNDS + index, &mut shaper);
            }
            started.elapsed().as_secs_f64() * 1e6 / (ROUNDS * LABELS) as f64
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    println!(
        "{} fonts: {:.2} us/label median, {:.2} min, {:.2} max (checksum {sink:.0})",
        if system { "system" } else { "registered" },
        samples[SAMPLES / 2],
        samples[0],
        samples[SAMPLES - 1],
    );
}
