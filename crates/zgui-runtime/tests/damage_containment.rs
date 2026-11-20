//! The compositor redraws nothing the paint walk did not visit.
//!
//! A window with the containment check on panics the first time a compositor frame redraws a
//! pixel outside the region its paint walk was gated on. Each case drives one kind of change
//! through a window drawing onto a renderer that keeps its target, so every frame's draw damage
//! is the compositor's own.

mod support;

use std::sync::Arc;

use compositor_api::Renderer;
use compositor_types::{RenderCapabilities, RenderTarget};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::{App, AppError, Runtime};
use zgui_testkit_compositor::{Frames, Kept};
use zgui_view::{BuildCx, IntoView, View};

/// A document of swatches, a line of text and a list long enough to scroll.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 20px }
                   .swatch { display: block; width: 40px; height: 20px; background-color: #303030 }
                   .swatch.lit { background-color: #c0c0c0 }
                   text { display: block; width: 200px }
                   .list { display: block; width: 300px; height: 120px; overflow: scroll;
                           background-color: #101010 }
                   .row { display: block; width: 280px; height: 20px; background-color: #202020 }";

/// A window over `css` with a lit swatch, a count, an optional extra row and a list, drawn onto a
/// kept target with the containment check on.
fn window(
    lit: RwSignal<bool>,
    count: RwSignal<i32>,
    extra: RwSignal<bool>,
) -> (Harness<Runtime>, Frames) {
    let frames = Frames::default();
    let handed = frames.clone();
    let handler = App::new()
        .with_title("containment")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_compositor(support::compositor())
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Kept::new(RenderCapabilities {
                    can_shift: true,
                    ..RenderCapabilities::MINIMAL
                })
                .recording_into(handed.clone());
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut list = zgui_elements::column().class("list");
            for _ in 0..40 {
                list = list.child(zgui_elements::column().class("row"));
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::column()
                            .class("swatch")
                            .class_toggle(zgui_view::ClassName::new("lit"), move || lit.get()),
                    )
                    .child(zgui_elements::text().child(move || count.get().to_string()))
                    .child(zgui_view::Show::new(
                        move || extra.get(),
                        || zgui_view::AnyView::new(zgui_elements::column().class("swatch")),
                    ))
                    .child(list)
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.app_mut().windows_mut()[0].set_check_damage_containment(true);
    harness.settle(8);
    (harness, frames)
}

#[test]
fn a_restyle_a_text_change_a_mount_and_a_scroll_redraw_only_what_the_walk_visited() {
    let (lit, count, extra) = (RwSignal::new(false), RwSignal::new(1), RwSignal::new(false));
    let (mut harness, frames) = window(lit, count, extra);
    frames.clear();

    lit.set(true);
    harness.settle(8);
    count.set(42);
    harness.settle(8);
    extra.set(true);
    harness.settle(8);
    harness.deliver_to_first(zgui_platform::SurfaceEvent::Wheel {
        event: zgui_vocab::WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: zgui_geom::Point::new(zgui_geom::CssPx(100.0), zgui_geom::CssPx(150.0)),
            delta: zgui_vocab::ScrollDelta::Pixels(zgui_geom::Size::new(
                zgui_geom::CssPx(0.0),
                zgui_geom::CssPx(60.0),
            )),
            phase: zgui_vocab::ScrollPhase::Discrete,
        },
        modifiers: zgui_vocab::Modifiers::NONE,
        timestamp: zgui_vocab::Timestamp::ORIGIN,
    });
    for _ in 0..30 {
        harness.advance(std::time::Duration::from_millis(16));
        harness.pump();
    }

    // The check ran over frames that redrew part of the output, which is all it reads.
    let partial = frames
        .drawn()
        .iter()
        .filter(|frame| !frame.damage.draw.is_full() && !frame.damage.draw.is_empty())
        .count();
    assert!(
        partial >= 3,
        "only {partial} frames redrew part of the output, so the check read almost nothing"
    );
}
