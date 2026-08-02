//! What a rasteriser holds, what it shares, and what happens when it writes nothing.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Point, Rect, Size};
use zgui_render::{Renderer, VectorRaster};
use zgui_render_vector_vello::{VelloRaster, device};
use zgui_scene::{ClipId, Paint, PaintRef, VectorId, VectorItem};

use support::{Which, harness, opaque, path, present, quad, rect, scene};

/// Everything on one device shares one path renderer, and two devices never do.
///
/// Not a micro-optimisation: the fixed buffers are measured in hundreds of megabytes, so one per
/// window would be a device's whole budget spent on copies of one thing.
#[test]
fn two_rasterisers_on_one_device_share_one_renderer() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let first = device::for_gpu(&gpu).expect("this device already built one");
    let second = device::for_gpu(&gpu).expect("and will not build a second");
    assert!(
        Arc::ptr_eq(&first, &second),
        "a second rasteriser on the same device built a second renderer"
    );

    // And two rasterisers really can be built over it, which is what the sharing is for.
    let one = VelloRaster::new(&gpu, 64, 64).expect("a rasteriser");
    let two = VelloRaster::new(&gpu, 64, 64).expect("a second rasteriser");
    assert_eq!(
        one.memory().fixed,
        two.memory().fixed,
        "both report the same fixed footprint, because it is the same renderer's"
    );
    assert!(
        one.memory().scratch > 0 && two.memory().scratch > 0,
        "each holds a scratch of its own, which is the part that does scale"
    );
}

/// The measured fixed footprint, printed so the figure in the budget has a source.
#[test]
fn the_fixed_footprint_is_reported_in_mebibytes() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    support::vector(
        &mut scene,
        0,
        path(rect(8.0, 8.0, 32.0, 32.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let outcome = harness.renderer.draw(&scene, &DamageSet::full());
    let memory = outcome.stats().expect("presented").memory;
    println!(
        "path renderer fixed: {:.1} MiB; scratch: {:.1} MiB",
        memory.fixed as f64 / (1024.0 * 1024.0),
        memory.scratch as f64 / (1024.0 * 1024.0),
    );
    assert!(memory.fixed > 0, "nothing measured the fixed footprint");
}

/// The pre-clear turns a rasterisation that wrote nothing into missing content, not wrong content.
///
/// The second frame reuses the same scratch layer at the same region, and one of its items paints
/// nothing this rasteriser can express — so nothing is written where the first frame's red square
/// was. Its composite still reads that part of the scratch, because the plan still gave the item an
/// ink rectangle. Without the pre-clear the frame would show the *previous* frame's square.
#[test]
fn a_reused_scratch_does_not_replay_the_previous_frame() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let square = rect(16.0, 16.0, 48.0, 48.0);

    let mut first = scene();
    quad(&mut first, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    support::vector(&mut first, 0, path(square), opaque(255, 0, 0), ClipId::ROOT);
    support::vector(
        &mut first,
        1,
        path(rect(72.0, 72.0, 40.0, 40.0)),
        opaque(0, 255, 0),
        ClipId::ROOT,
    );
    first.finish(&DamageSet::full());
    let drawn = present(&mut harness.renderer, &first);
    assert_eq!(
        drawn.rgba(40, 40),
        [255, 0, 0, 255],
        "the first frame drew it"
    );

    // The same two items, but the first one now asks for a paint nothing here can draw. It keeps
    // its ink, so its composite still reads the same corner of the same layer.
    let mut second = scene();
    quad(&mut second, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    let unpaintable = second.paints.add(Paint::Image {
        tile: zgui_atlas::AtlasTile {
            texture: zgui_atlas::TextureId::new(zgui_atlas::TextureKind::Color, 0),
            tile: zgui_atlas::TileId(0),
            bounds: Rect::new(Point::new(0, 0), Size::new(8, 8)),
        },
        destination: square,
        transform: zgui_scene::SpatialId::VIEWPORT,
        repeating: false,
    });
    second.push_vector(VectorItem::filled(VectorId(0), path(square), unpaintable));
    support::vector(
        &mut second,
        1,
        path(rect(72.0, 72.0, 40.0, 40.0)),
        opaque(0, 255, 0),
        ClipId::ROOT,
    );
    second.finish(&DamageSet::full());
    assert_eq!(
        second.pass_plan().items.len(),
        2,
        "the unpaintable item still has an ink rectangle, so its composite still reads the scratch"
    );

    let replayed = present(&mut harness.renderer, &second);
    assert_eq!(
        replayed.rgba(40, 40),
        [0, 0, 0, 255],
        "the reused layer replayed the previous frame's square instead of reading as cleared"
    );
    assert_eq!(
        replayed.rgba(92, 92),
        [0, 255, 0, 255],
        "the item that could be painted still was, so the composite really did run"
    );
}

/// A path with neither fill nor stroke costs nothing and draws nothing.
#[test]
fn an_unpainted_path_draws_nothing() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(10, 20, 30));
    scene.push_vector(VectorItem::filled(
        VectorId(0),
        path(rect(16.0, 16.0, 64.0, 64.0)),
        PaintRef::NONE,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut harness.renderer, &scene);
    assert_eq!(pixels.rgba(48, 48), [10, 20, 30, 255]);
}

/// The rasteriser a device is given is the one its own capabilities choose.
///
/// The probe and the branch it selects land together, so a device that cannot run the path renderer
/// has a rasteriser from here on rather than an empty window and a note to write one later.
#[test]
fn the_rasteriser_a_device_gets_is_the_one_its_capabilities_choose() {
    let Some(harness) = harness(Which::Vello) else {
        return;
    };
    let gpu = Arc::clone(harness.gpu());
    let capable = zgui_render::Renderer::capabilities(&*harness).vector_compute;
    let chosen = zgui_render_vector_vello::chosen(&gpu);
    assert_eq!(
        chosen == zgui_render_vector_vello::Choice::Compute,
        capable,
        "the choice and the capability it is read from disagree"
    );

    // And whichever it is, something is bound and it can rasterise: the fallback is a branch that
    // runs, not a branch that exists.
    let mut renderer = harness;
    let raster = zgui_render_vector_vello::for_device(&gpu, Size::new(128, 128));
    renderer.set_vector_raster(raster);
    assert!(renderer.has_vector_raster());

    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    support::vector(
        &mut scene,
        0,
        path(rect(32.0, 32.0, 64.0, 64.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer.renderer, &scene);
    assert_eq!(pixels.rgba(64, 64), [255, 255, 255, 255]);
}

/// A frame drawn with no rasteriser attached counts its composites rather than drawing them.
#[test]
fn a_renderer_with_no_rasteriser_draws_no_vector_content_and_says_so() {
    let guard = support::device_lock();
    let target = zgui_render::RenderTarget::new(Size::new(128, 128), zgui_geom::Scale::new(1.0));
    let Ok(mut renderer) = zgui_render_wgpu::Builder::new().offscreen(
        target,
        zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm,
        false,
    ) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    assert!(!renderer.has_vector_raster());

    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(0, 0, 0));
    support::vector(
        &mut scene,
        0,
        path(rect(32.0, 32.0, 64.0, 64.0)),
        opaque(255, 255, 255),
        ClipId::ROOT,
    );
    scene.finish(&DamageSet::full());
    let outcome = renderer.draw(&scene, &DamageSet::full());
    let stats = outcome.stats().expect("presented");
    assert_eq!(
        stats.vector_passes, 1,
        "the display list still planned the pass, which is what makes the count assertable \
         without a device that can run one"
    );
    let pixels = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        pixels.rgba(64, 64),
        [0, 0, 0, 255],
        "with nothing to rasterise it, the composite must draw nothing rather than draw whatever \
         a scratch that was never written happens to hold"
    );
    drop(guard);
}
