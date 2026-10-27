//! The format a group's target is held in.
//!
//! A plain group (no filter, source-over, in no other group) is held in the composed format. What
//! it holds keeps its own targets and composites into it. Every other group keeps `Rgba16Float`,
//! and draws exactly as before.
//!
//! Tolerance of a plain group against the same group in `Rgba16Float`: each blend into its eight-bit
//! target rounds by up to half a step. Content with at most one translucent layer on a pixel
//! differs by at most one step in each channel. Translucent layers stacked inside the group, and
//! groups and backdrops inside it, add their roundings and differ by at most two steps. Content
//! drawn with no group rounds the same blends in the composed target. At full opacity a plain
//! group stays as close to that picture as a group in `Rgba16Float` does.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_render::Renderer;
use zgui_render_wgpu::Pixels;
use zgui_scene::peniko::{BlendMode, Compose, Mix};
use zgui_scene::{BackdropFilter, Filter, GroupBoundary, Quad, Scene};

use support::{SIDE, plain_renderer, present, rect, renderer_pair};

/// A quad of `color` filling `bounds`, with every corner rounded by `radius`.
fn quad(scene: &mut Scene, bounds: (f32, f32, f32, f32), color: Color, radius: f32) {
    let paint = scene.paints.add(zgui_scene::Paint::Solid(color));
    let mut quad = Quad::filled(rect(bounds.0, bounds.1, bounds.2, bounds.3), paint);
    quad.radii = [radius; 8];
    scene.push_quad(quad);
}

/// Wraps `content` in a group of `opacity` that composites by `blend` through `filters`.
fn grouped(
    scene: &mut Scene,
    bounds: (f32, f32, f32, f32),
    opacity: f32,
    blend: BlendMode,
    filters: &[Filter],
    content: impl FnOnce(&mut Scene),
) {
    let boundary = GroupBoundary::start(
        rect(bounds.0, bounds.1, bounds.2, bounds.3),
        opacity,
        blend,
        filters.iter().copied().collect(),
    );
    scene.push_group(boundary.clone());
    content(scene);
    scene.push_group(boundary.end());
}

/// A translucent colour from four bytes.
fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
    Color::srgb_u8(red, green, blue, alpha)
}

/// A scene with a backdrop of `background` and `content` drawn over it.
fn scene_of(background: Color, content: impl FnOnce(&mut Scene)) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(
        &mut scene,
        (0.0, 0.0, SIDE as f32, SIDE as f32),
        background,
        0.0,
    );
    content(&mut scene);
    scene.finish(&DamageSet::full());
    scene
}

/// Content that stacks `layers` translucent, rounded and off-grid quads.
fn stacked(scene: &mut Scene, layers: u8, alpha: u8) {
    for layer in 0..layers {
        let at = 10.3 + 7.7 * f32::from(layer);
        quad(
            scene,
            (at, at * 0.8, 61.4, 47.9),
            rgba(
                40u8.wrapping_add(layer.wrapping_mul(53)),
                200u8.wrapping_sub(layer.wrapping_mul(31)),
                90u8.wrapping_add(layer.wrapping_mul(17)),
                alpha,
            ),
            9.5,
        );
    }
}

/// Scenes whose only group is plain and holds at most one translucent layer on a pixel.
fn single_layer_scenes() -> Vec<(&'static str, Scene)> {
    let normal = BlendMode::default();
    vec![
        (
            "half opacity over two overlapping opaque quads",
            scene_of(rgba(200, 120, 60, 255), |scene| {
                grouped(scene, (16.0, 16.0, 96.0, 96.0), 0.5, normal, &[], |scene| {
                    quad(scene, (16.5, 16.5, 64.0, 64.0), rgba(10, 20, 30, 255), 12.0);
                    quad(
                        scene,
                        (48.25, 48.25, 64.0, 64.0),
                        rgba(240, 250, 5, 255),
                        12.0,
                    );
                });
            }),
        ),
        (
            "a third of opacity over one translucent quad",
            scene_of(rgba(30, 60, 220, 255), |scene| {
                grouped(
                    scene,
                    (0.0, 0.0, 128.0, 128.0),
                    0.33,
                    normal,
                    &[],
                    |scene| {
                        stacked(scene, 1, 150);
                    },
                );
            }),
        ),
        (
            "a translucent group of one quad over a translucent surface",
            scene_of(rgba(120, 30, 90, 140), |scene| {
                grouped(
                    scene,
                    (8.0, 8.0, 100.0, 100.0),
                    0.61,
                    normal,
                    &[],
                    |scene| {
                        stacked(scene, 1, 128);
                    },
                );
            }),
        ),
    ]
}

/// Plain groups of `layers` stacked translucent quads of `alpha`, at `opacity`.
fn stacked_scene(layers: u8, alpha: u8, opacity: f32) -> Scene {
    scene_of(rgba(250, 250, 250, 255), |scene| {
        grouped(
            scene,
            (0.0, 0.0, 128.0, 128.0),
            opacity,
            BlendMode::default(),
            &[],
            |scene| stacked(scene, layers, alpha),
        );
    })
}

/// The scenes whose groups need headroom, which keep `Rgba16Float`.
fn wide_scenes() -> Vec<(&'static str, Scene)> {
    let normal = BlendMode::default();
    let bounds = (8.0, 8.0, 112.0, 112.0);
    vec![
        (
            "a blurred group",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(scene, bounds, 0.5, normal, &[Filter::Blur(3.0)], |scene| {
                    stacked(scene, 3, 150);
                });
            }),
        ),
        (
            "a colour-matrix group",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(
                    scene,
                    bounds,
                    0.5,
                    normal,
                    &[Filter::Brightness(1.3)],
                    |scene| stacked(scene, 3, 150),
                );
            }),
        ),
        (
            "a blend-mode group",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(
                    scene,
                    bounds,
                    0.5,
                    BlendMode::new(Mix::Multiply, Compose::SrcOver),
                    &[],
                    |scene| stacked(scene, 3, 150),
                );
            }),
        ),
    ]
}

/// Plain groups that hold a group, a filtered group or a backdrop.
fn holding_scenes() -> Vec<(&'static str, Scene)> {
    let normal = BlendMode::default();
    let bounds = (8.0, 8.0, 112.0, 112.0);
    vec![
        (
            "nested groups",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(scene, bounds, 0.5, normal, &[], |scene| {
                    grouped(scene, bounds, 0.5, normal, &[], |scene| {
                        stacked(scene, 3, 150);
                    });
                });
            }),
        ),
        (
            "a group holding a blurred group",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(scene, bounds, 0.7, normal, &[], |scene| {
                    stacked(scene, 2, 150);
                    grouped(scene, bounds, 0.6, normal, &[Filter::Blur(3.0)], |scene| {
                        stacked(scene, 3, 150);
                    });
                });
            }),
        ),
        (
            "a group holding a backdrop",
            scene_of(rgba(255, 255, 255, 255), |scene| {
                grouped(scene, bounds, 0.5, normal, &[], |scene| {
                    stacked(scene, 3, 150);
                    scene.push_backdrop(BackdropFilter::new(
                        rect(30.0, 30.0, 50.0, 40.0),
                        [Filter::Blur(4.0)].into_iter().collect(),
                    ));
                });
            }),
        ),
    ]
}

/// The largest difference in any channel, and how many pixels differ at all.
fn compare(narrow: &Pixels, wide: &Pixels) -> (u8, usize) {
    let mut largest = 0;
    let mut differing = 0;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let (a, b) = (narrow.rgba(x, y), wide.rgba(x, y));
            let delta = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            largest = largest.max(delta);
            differing += usize::from(delta > 0);
        }
    }
    (largest, differing)
}

#[test]
fn a_plain_group_of_one_translucent_layer_differs_by_at_most_one_step() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    for (name, scene) in single_layer_scenes() {
        let (held, reference) = (present(&mut narrow, &scene), present(&mut wide, &scene));
        let (largest, differing) = compare(&held, &reference);
        eprintln!("{name}: max channel delta {largest}, {differing} pixels differ");
        assert!(
            largest <= 1,
            "{name}: a plain group in eight bits differs by {largest} steps from one in sixteen"
        );
    }
}

/// Stacked translucent layers round once for each blend into an eight-bit target. At full opacity
/// a plain group is never further from the picture with no group than a sixteen-bit one is.
#[test]
fn stacked_layers_in_a_plain_group_differ_by_at_most_two_steps() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    for (layers, alpha, opacity) in [(2, 150, 1.0), (3, 150, 0.33), (8, 77, 1.0), (8, 201, 0.77)] {
        let scene = stacked_scene(layers, alpha, opacity);
        let (held, reference) = (present(&mut narrow, &scene), present(&mut wide, &scene));
        let (largest, differing) = compare(&held, &reference);
        eprintln!(
            "{layers} layers of alpha {alpha} at opacity {opacity}: max channel delta {largest}, \
             {differing} pixels differ"
        );
        assert!(
            largest <= 2,
            "{layers} layers: a plain group in eight bits differs by {largest} steps"
        );
        if opacity < 1.0 {
            continue;
        }
        // At full opacity a group draws the same picture as no group, up to rounding.
        let direct = scene_of(rgba(250, 250, 250, 255), |scene| {
            stacked(scene, layers, alpha);
        });
        let direct = present(&mut narrow, &direct);
        let (from_narrow, _) = compare(&held, &direct);
        let (from_wide, _) = compare(&reference, &direct);
        assert!(
            from_narrow <= from_wide,
            "{layers} layers: eight bits are {from_narrow} steps from no group, sixteen {from_wide}"
        );
    }
}

#[test]
fn a_group_that_needs_headroom_draws_exactly_as_before() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    for (name, scene) in wide_scenes() {
        let (held, reference) = (present(&mut narrow, &scene), present(&mut wide, &scene));
        assert_eq!(
            held.max_difference(&reference),
            0,
            "{name}: a group that needs headroom keeps its sixteen-bit target"
        );
        assert_eq!(
            narrow.groups().bytes(),
            wide.groups().bytes(),
            "{name}: and the pool holds the same bytes"
        );
    }
}

/// What a plain group holds keeps its own target and composites into the plain group's target.
#[test]
fn a_plain_group_holding_groups_and_backdrops_differs_by_at_most_two_steps() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    for (name, scene) in holding_scenes() {
        let (held, reference) = (present(&mut narrow, &scene), present(&mut wide, &scene));
        let (largest, differing) = compare(&held, &reference);
        eprintln!("{name}: max channel delta {largest}, {differing} pixels differ");
        assert!(
            largest <= 2,
            "{name}: a plain group in eight bits differs by {largest} steps from one in sixteen"
        );
    }
}

/// Two groups over one region: the outer one takes four bytes a texel, and the inner one, which is
/// in another group, keeps eight.
#[test]
fn a_group_in_a_plain_group_keeps_its_wide_target() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    let (_, nested) = holding_scenes().swap_remove(0);
    present(&mut narrow, &nested);
    present(&mut wide, &nested);
    assert_eq!(narrow.groups().bytes() * 4, wide.groups().bytes() * 3);
}

#[test]
fn a_plain_group_costs_half_the_bytes_of_a_wide_one() {
    let Some((mut narrow, mut wide)) = renderer_pair() else {
        return;
    };
    wide.set_narrow_groups(false);
    let (_, scene) = single_layer_scenes().swap_remove(0);
    present(&mut narrow, &scene);
    present(&mut wide, &scene);
    assert!(narrow.groups().bytes() > 0, "the group was isolated");
    assert_eq!(
        narrow.groups().bytes() * 2,
        wide.groups().bytes(),
        "four bytes a texel against eight, over the same region"
    );
}

/// An animated plain group, drawn over the damage each step produces, takes one texture and
/// keeps it: the format does not change from frame to frame.
#[test]
fn an_animated_plain_group_keeps_one_texture_under_partial_damage() {
    let Some(mut animated) = plain_renderer() else {
        return;
    };
    let bounds = |phase: f32| (40.0 - 20.0 * phase, 30.0, 41.0, 18.0);
    let frame = |scene: &mut Scene, phase: f32, damage: &DamageSet| {
        scene.begin_frame(Size::new(SIDE, SIDE));
        quad(
            scene,
            (0.0, 0.0, SIDE as f32, SIDE as f32),
            rgba(255, 255, 255, 255),
            0.0,
        );
        let at = bounds(phase);
        grouped(
            scene,
            at,
            0.4 + 0.6 * phase,
            BlendMode::default(),
            &[],
            |scene| {
                quad(scene, at, rgba(40, 120, 200, 255), 4.0);
                quad(
                    scene,
                    (at.0 + 8.0, at.1 + 6.0, 12.0, 6.0),
                    rgba(255, 255, 255, 160),
                    0.0,
                );
            },
        );
        scene.finish(damage);
    };
    let damage_of = |from: f32, to: f32| {
        let mut damage = DamageSet::new();
        for phase in [from, to] {
            let (x, y, w, h) = bounds(phase);
            damage.absorb(Rect::<i32, Device>::new(
                Point::new(x.floor() as i32, y.floor() as i32),
                Size::new(w.ceil() as i32 + 1, h.ceil() as i32 + 1),
            ));
        }
        damage
    };
    let mut scene = Scene::new();
    frame(&mut scene, 0.0, &DamageSet::full());
    present(&mut animated, &scene);
    let (allocations, bytes) = (animated.groups().allocations(), animated.groups().bytes());
    for step in 1..=20 {
        let (from, to) = (0.9 * (step - 1) as f32 / 20.0, 0.9 * step as f32 / 20.0);
        frame(&mut scene, to, &damage_of(from, to));
        let outcome = animated.draw(&scene, &damage_of(from, to));
        assert!(outcome.stats().is_some());
        assert_eq!(
            animated.groups().allocations(),
            allocations,
            "step {step} allocated a texture"
        );
        assert_eq!(animated.groups().bytes(), bytes, "step {step}");
    }
}

/// The largest difference in any channel inside `within`.
fn difference_in(first: &Pixels, second: &Pixels, within: Rect<i32, Device>) -> u8 {
    let mut largest = 0;
    for y in within.top()..within.bottom() {
        for x in within.left()..within.right() {
            let (a, b) = (first.rgba(x, y), second.rgba(x, y));
            for channel in 0..4 {
                largest = largest.max(a[channel].abs_diff(b[channel]));
            }
        }
    }
    largest
}

/// A translucent panel with stacked layers at its top left and, when `corner` is set, a nested
/// group and a backdrop at its right edge.
///
/// A frame whose damage misses the right edge emits the panel without them, as the paint walk does.
fn panel(corner: bool, damage: &DamageSet) -> Scene {
    let normal = BlendMode::default();
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(
        &mut scene,
        (0.0, 0.0, SIDE as f32, SIDE as f32),
        rgba(250, 250, 250, 255),
        0.0,
    );
    grouped(
        &mut scene,
        (0.0, 0.0, 128.0, 128.0),
        0.6,
        normal,
        &[],
        |scene| {
            stacked(scene, 4, 150);
            if corner {
                grouped(scene, (96.0, 96.0, 28.0, 28.0), 0.5, normal, &[], |scene| {
                    quad(scene, (96.0, 96.0, 28.0, 28.0), rgba(20, 40, 200, 255), 3.0);
                });
                scene.push_backdrop(BackdropFilter::new(
                    rect(100.0, 40.0, 24.0, 20.0),
                    [Filter::Blur(2.0)].into_iter().collect(),
                ));
            }
        },
    );
    scene.finish(damage);
    scene
}

/// A partial frame whose damage misses what a group holds draws the group without it. The group
/// keeps its format, so the frame reuses the group's texture and draws the pixels a whole frame
/// does.
#[test]
fn a_group_keeps_its_format_when_the_damage_leaves_out_what_it_holds() {
    let Some((mut whole, mut partial)) = renderer_pair() else {
        return;
    };
    let reference = present(&mut whole, &panel(true, &DamageSet::full()));
    present(&mut partial, &panel(true, &DamageSet::full()));
    let (allocations, bytes) = (partial.groups().allocations(), partial.groups().bytes());

    let within = Rect::<i32, Device>::new(Point::new(10, 8), Size::new(60, 50));
    let mut damage = DamageSet::new();
    damage.absorb(within);
    let outcome = partial.draw(&panel(false, &damage), &damage);
    assert!(outcome.stats().is_some());
    let pixels = partial
        .read_presented()
        .expect("a stand-in surface can be read back");
    assert_eq!(
        partial.groups().allocations(),
        allocations,
        "the partial frame allocated a texture of another format"
    );
    assert_eq!(partial.groups().bytes(), bytes);
    assert_eq!(
        difference_in(&reference, &pixels, within),
        0,
        "a partial redraw differs from a whole one inside {within:?}"
    );
}
