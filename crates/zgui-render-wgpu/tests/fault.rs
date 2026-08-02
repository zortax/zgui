//! Driving every answer a surface can give, and checking what the next frame shows.
//!
//! Six of the seven are ordinary events in a window's life and none of them happens on demand, so
//! the paths that handle them are only ever taken in a test by asking for them. The property being
//! checked is one sentence: **damage is retired when a frame's work was submitted, not when a
//! frame was presented** — so after any run of bad answers, the next frame that does present has
//! to be the frame, and not the parts of it that happened to be damaged since.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Point, Rect, Size};
use zgui_render::{FrameOutcome, Renderer};
use zgui_render_wgpu::{Acquisition, wgpu};
use zgui_scene::{Quad, Scene};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// A scene whose one square is at `step` pixels along.
fn moving_square(step: i32) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    let red = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(220, 30, 30)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    scene.push_quad(Quad::filled(
        rect(8.0 + step as f32 * 8.0, 40.0, 32.0, 32.0),
        red,
    ));
    scene.finish(&DamageSet::full());
    scene
}

/// The rectangle the square moved through between two steps.
fn moved(before: i32, after: i32) -> DamageSet {
    let mut damage = DamageSet::<4>::new();
    for step in [before, after] {
        damage.absorb(Rect::new(Point::new(7 + step * 8, 39), Size::new(34, 34)));
    }
    damage
}

#[test]
fn every_answer_a_surface_can_give_is_driven_and_none_of_them_loses_a_frame() {
    let Some((mut faulted, mut clean)) = support::renderer_pair() else {
        return;
    };

    for injected in Acquisition::ALL {
        // Both start from the same picture.
        let start = moving_square(0);
        let expected = present(&mut clean, &start);
        assert_eq!(present(&mut faulted, &start).max_difference(&expected), 0);

        // A frame that gets the bad answer. Its work is submitted whatever the answer was, so its
        // damage is retired — which is the sentence being tested, and the reason the next frame
        // may damage only what *it* changed.
        faulted.inject_surface_fault(injected, 1);
        let outcome = faulted.draw(&moving_square(1), &moved(0, 1));
        assert!(
            outcome.retires_damage(),
            "{injected:?} lost its frame's damage",
        );
        if injected == Acquisition::Lost {
            assert_eq!(outcome, FrameOutcome::Recovered, "{injected:?}");
        }

        // And the next frame, which damages only what it changed, is the whole picture.
        let after = faulted.draw(&moving_square(2), &moved(1, 2));
        assert!(
            after.stats().is_some() || injected == Acquisition::Lost,
            "{after:?}"
        );
        let recovered = faulted
            .read_presented()
            .expect("a stand-in surface can be read back");
        let reference = present(&mut clean, &moving_square(2));
        assert_eq!(
            recovered.max_difference(&reference),
            0,
            "after {injected:?} the next frame was not the frame drawn whole"
        );
    }
}

#[test]
fn an_injection_that_does_not_present_leaves_the_screen_showing_the_previous_frame() {
    // The other half of the same rule: a frame whose acquisition failed updated the target it
    // composed into, but nothing reached the screen, so what is on the screen is still the frame
    // before it. Asserting this is what stops "damage is retired anyway" being read as "the frame
    // was shown anyway".
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let first = present(&mut renderer, &moving_square(0));

    renderer.inject_surface_fault(Acquisition::Timeout, 1);
    let outcome = renderer.draw(&moving_square(1), &moved(0, 1));
    assert!(matches!(outcome, FrameOutcome::Skipped(_)), "{outcome:?}");
    let presented = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        presented.max_difference(&first),
        0,
        "nothing was copied to the surface, so it still shows the frame before"
    );
    let composed = renderer.read_composed();
    assert!(
        composed.max_difference(&first) > 0,
        "but the target the frame composed into did move on, which is why its damage is retired"
    );
}

#[test]
fn a_run_of_bad_answers_still_ends_in_the_right_picture() {
    // Several in a row, which is what a compositor that has stopped handing textures over looks
    // like from here. The count is part of the injector for exactly this case.
    let Some((mut faulted, mut clean)) = support::renderer_pair() else {
        return;
    };
    present(&mut faulted, &moving_square(0));

    faulted.inject_surface_fault(Acquisition::Occluded, 3);
    for step in 1..4 {
        let outcome = faulted.draw(&moving_square(step), &moved(step - 1, step));
        assert!(outcome.retires_damage());
        assert!(!outcome.wants_another_frame(), "an occluded window parks");
    }
    let outcome = faulted.draw(&moving_square(4), &moved(3, 4));
    assert!(
        outcome.stats().is_some(),
        "the injection ran out: {outcome:?}"
    );

    let recovered = faulted
        .read_presented()
        .expect("a stand-in surface can be read back");
    let reference = present(&mut clean, &moving_square(4));
    assert_eq!(recovered.max_difference(&reference), 0);
}

#[test]
fn an_external_texture_is_drawn_where_its_quad_says_and_clipped_like_everything_else() {
    // The one primitive whose pixels the renderer did not produce. What it shares with the rest is
    // the clip: the same chain, evaluated by the same function, so a video inside a rounded card
    // is clipped exactly as the card's own background is.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let gpu = std::sync::Arc::clone(renderer.gpu());
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test.external"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Solid opaque blue, so a magnifying read of it is blue everywhere.
    gpu.queue().write_texture(
        texture.as_image_copy(),
        &[0, 0, 255, 255].repeat(4),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );

    let id = zgui_scene::ExternalTextureId(7);
    renderer.register_external(zgui_render::ExternalTexture {
        id,
        handle: zgui_render::TextureHandle(0),
        size: Size::new(2, 2),
        premultiplied: true,
    });
    assert!(
        renderer.attach_external(id, &texture),
        "a described texture accepts its resource"
    );
    assert!(
        !renderer.attach_external(zgui_scene::ExternalTextureId(99), &texture),
        "and one nobody described has nothing to be drawn with"
    );

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let white = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 255, 255)));
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        white,
    ));
    let clip = scene
        .clips
        .only(zgui_scene::ClipLink::rect(rect(32.0, 32.0, 32.0, 64.0)));
    scene.push_external(
        zgui_scene::ExternalQuad::new(rect(32.0, 32.0, 64.0, 64.0), id).clipped(clip),
    );
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);
    assert_eq!(pixels.rgba(40, 40), [0, 0, 255, 255], "inside the clip");
    assert_eq!(
        pixels.rgba(80, 40),
        [255, 255, 255, 255],
        "outside the clip, where the quad reaches but the chain does not admit it"
    );
    assert_eq!(
        pixels.rgba(16, 16),
        [255, 255, 255, 255],
        "outside the quad"
    );
}

#[test]
fn an_answer_that_changed_the_surface_makes_the_next_frame_redraw_all_of_it() {
    // Two of the seven answers say the surface is no longer the one the frame was composed
    // against, and nothing observed what the compositor did with it in between — so what the
    // composed target holds is no longer known to be on screen and the next frame cannot redraw
    // only its own damage. The negative half is what stops this being an assertion about nothing:
    // a suboptimal texture was still presented, so the frame after it redraws its damage alone.
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let whole = u64::from(SIDE as u32) * u64::from(SIDE as u32);
    for (injected, forces) in [
        (Acquisition::Suboptimal, false),
        (Acquisition::Outdated, true),
    ] {
        present(&mut renderer, &moving_square(0));
        renderer.inject_surface_fault(injected, 1);
        renderer.draw(&moving_square(1), &moved(0, 1));

        let next = renderer.draw(&moving_square(2), &moved(1, 2));
        let redrawn = next
            .stats()
            .map(|stats| stats.damage_px)
            .expect("the injection ran out, so this frame presented");
        assert_eq!(
            redrawn == whole,
            forces,
            "after {injected:?} the next frame redrew {redrawn} of {whole} device pixels"
        );
    }
}
