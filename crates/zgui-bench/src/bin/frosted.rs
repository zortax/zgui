//! A diagnostic probe: what a frosted card in a frosted panel holds on the device.
//!
//! **This is a diagnostic probe, not a reference workload.** It gates nothing.
//!
//! A field of coloured, labelled tiles fills the window. Over it sits a panel with a
//! `backdrop-filter`, in the panel a card with a `backdrop-filter` of its own, and in the card a
//! group whose opacity a keyframed animation moves. The window always draws on this machine's
//! device, offscreen, at scale one.
//!
//! After the first frame and again after thirty animated steps, the probe prints what
//! [`Renderer::memory`](zgui::render::Renderer::memory) reports, what the group pool holds and the
//! most targets it lent at once, and the totals of the device allocator's report. The animated
//! steps are printed as the other probes print them, with GPU time under `ZGUI_GPU_TIMING=1`.
//!
//! ```text
//! cargo build --release -p zgui-bench --bin frosted
//! target/release/frosted                 # 1920x1080 and 3840x2160, each in a process of its own
//! ZGUI_GPU_TIMING=1 target/release/frosted 3840x2160
//! ```

#![forbid(unsafe_code)]

use zgui::view;
use zgui::view::{Anchor, AnyView, BuildCx, IntoView, View};
use zgui_bench::probe::{self, Device};
use zgui_platform_headless::Harness;
use zgui_render_wgpu::{GroupPool, WgpuRenderer};

/// Steps run before the reported ones, past the first paint.
const WARM: usize = 8;

/// Steps reported.
const STEPS: usize = 30;

/// One tile of the field.
const TILE: (f32, f32) = (120.0, 90.0);

/// The sheet for a window `width` by `height`.
fn sheet(width: f32, height: f32) -> String {
    let (panel_w, panel_h) = (width * 0.8, height * 0.8);
    let (card_w, card_h) = (panel_w * 0.5, panel_h * 0.5);
    format!(
        "
        :root {{ background-color: #0b0d12; color: #f0f3f8; font-family: \"Noto Sans\"; font-size: 14px }}
        .scene {{ position: relative; width: {width}px; height: {height}px }}
        .field {{ display: flex; flex-direction: row; flex-wrap: wrap; width: {width}px;
                  height: {height}px; overflow: hidden }}
        .tile {{ width: {tile_w}px; height: {tile_h}px; background-color: #1f6feb }}
        .tile.b {{ background-color: #f0883e }}
        .tile.c {{ background-color: #2ea043 }}
        .panel {{ position: absolute; left: {panel_x}px; top: {panel_y}px; width: {panel_w}px;
                  height: {panel_h}px; border-radius: 24px;
                  background-color: rgba(255, 255, 255, 0.08); backdrop-filter: blur(24px) }}
        .card {{ position: absolute; left: {card_x}px; top: {card_y}px; width: {card_w}px;
                 height: {card_h}px; border-radius: 16px; padding: 24px;
                 background-color: rgba(255, 255, 255, 0.10); backdrop-filter: blur(12px) }}
        @keyframes probe-pulse {{
            from {{ opacity: 0.85 }}
            50% {{ opacity: 0.4 }}
            to {{ opacity: 0.85 }}
        }}
        .group {{ display: block; width: {group_w}px; padding: 16px; border-radius: 12px;
                  background-color: rgba(20, 24, 32, 0.6); opacity: 0.85;
                  animation-name: probe-pulse; animation-duration: 1000ms;
                  animation-timing-function: linear; animation-iteration-count: infinite }}
        .line {{ height: 22px }}
        .bar {{ width: 70%; height: 10px; margin-top: 8px; border-radius: 5px;
                background-color: #58a6ff }}",
        tile_w = TILE.0,
        tile_h = TILE.1,
        panel_x = width * 0.1,
        panel_y = height * 0.1,
        card_x = panel_w * 0.1,
        card_y = panel_h * 0.1,
        group_w = card_w * 0.6,
    )
}

/// The field, the panel, the card and the group, for a window `width` by `height`.
fn scene(width: f32, height: f32) -> impl IntoView {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a tile count of a window at most a few thousand pixels across"
    )]
    let tiles = ((width / TILE.0).ceil() * (height / TILE.1).ceil()) as usize;
    let field: Vec<AnyView> = (0..tiles)
        .map(|index| {
            let class = match index % 3 {
                0 => "tile",
                1 => "tile b",
                _ => "tile c",
            };
            AnyView::new(
                view! { box(class = class) { label(class = "name") {{format!("tile {index}")}} } },
            )
        })
        .collect();
    let lines: Vec<AnyView> = (0..4)
        .map(|index| {
            AnyView::new(view! { label(class = "line") {{format!("frosted line {index}")}} })
        })
        .collect();
    view! {
        box(class = "scene") {
            box(class = "field") { {field} }
            box(class = "panel") {
                box(class = "card") {
                    box(class = "group") {
                        {lines}
                        box(class = "bar")
                    }
                }
            }
        }
    }
}

/// Prints one `MEM` line: the renderer's report, the group pool and the device allocator.
fn memory(stage: &str, harness: &Harness<zgui::runtime::Runtime>, size: (f32, f32)) {
    let renderer = harness.app().windows()[0].renderer();
    let memory = renderer.memory();
    let pool = renderer.target_pool();
    let Some(device) = renderer
        .as_any()
        .and_then(|any| any.downcast_ref::<WgpuRenderer>())
    else {
        println!("MEM\t{stage}\tno device renderer");
        return;
    };
    let groups = device.groups();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a surface extent in device pixels"
    )]
    let full_target = (size.0 as u64) * (size.1 as u64) * bytes_per_texel(GroupPool::FORMAT);
    let report = device.gpu().device().generate_allocator_report();
    println!(
        "MEM\t{stage}\tfixed={}\ttargets={}\tscratch={}\tatlases={}\tbuffers={}\ttotal={}\t\
         pool_resident={}\tpool_peak_lent_targets={}\tpool_peak_lent_bytes_full_res={}\t\
         pool_degraded={}\tpool_leases={}\talloc_allocated={}\talloc_reserved={}\t\
         alloc_blocks={}\talloc_allocations={}",
        memory.fixed,
        memory.targets,
        memory.scratch,
        memory.atlases,
        memory.buffers,
        memory.total(),
        pool.resident,
        groups.peak(),
        u64::from(groups.peak()) * full_target,
        groups.degraded(),
        pool.leases,
        or_dash(report.as_ref().map(|report| report.total_allocated_bytes)),
        or_dash(report.as_ref().map(|report| report.total_reserved_bytes)),
        or_dash(report.as_ref().map(|report| report.blocks.len() as u64)),
        or_dash(
            report
                .as_ref()
                .map(|report| report.allocations.len() as u64)
        ),
    );
}

/// The bytes one texel of `format` takes.
fn bytes_per_texel(format: zgui_render_wgpu::wgpu::TextureFormat) -> u64 {
    format.block_copy_size(None).map_or(0, u64::from)
}

/// A count to print, or `-` when the device gave none.
fn or_dash(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| value.to_string())
}

fn main() {
    let variant = probe::variant_or_each(&["1920x1080", "3840x2160"]);
    let Some((width, height)) = variant
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
    else {
        eprintln!("usage: frosted [WIDTHxHEIGHT], not `{variant}`");
        std::process::exit(2);
    };
    let name = format!("frosted/{variant}");
    let opened = probe::open(
        "frosted",
        width,
        height,
        &sheet(width, height),
        Device::Gpu,
        move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(scene(width, height).into_view().build(cx))
        },
    );
    let mut opened = opened.unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let describe = opened.harness.app().windows()[0]
        .renderer()
        .as_any()
        .and_then(|any| any.downcast_ref::<WgpuRenderer>())
        .map_or_else(|| "-".to_owned(), |device| device.gpu().describe());
    println!("PROBE\t{name}\tdevice={describe}");
    memory("first", &opened.harness, (width, height));
    probe::census(&name, &opened);
    probe::step(&mut opened, WARM);
    let steps = probe::step(&mut opened, STEPS);
    memory("animated", &opened.harness, (width, height));
    let probe::Opened { mut harness, draws } = opened;
    harness.shut_down();
    // The renderer drops with the harness, which waits for the last GPU results.
    drop(harness);
    probe::report(&name, &steps, &draws.borrow());
}
