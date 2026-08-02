//! The scenes the coalescing policy is measured on.
//!
//! Seven shapes, chosen because they are the ones real interfaces produce and because they disagree
//! with each other about the policy:
//!
//! * a twenty-region chart dashboard with a legend drawn **over** each chart — the case where
//!   z-order genuinely requires one pass per region, and the case the three readings of rule 3
//!   disagree most sharply about;
//! * the same dashboard with the legends drawn **under** the charts, which needs one pass;
//! * a twelve-avatar row, where every item has its own rounded clip and nothing is drawn between
//!   them — the case clip absorption exists for;
//! * a stacked-area chart, where each band overlaps the one before it — the case a per-item
//!   composite is *not* sound for;
//! * two cards side by side nesting their drawings to different depths, where draw order *falls*
//!   as the frame is emitted — the case that decides where a pass's one composite belongs;
//! * those same two cards with a badge painted over the lower drawing once both have been emitted
//!   — the case one composite cannot serve at all;
//! * a row of drawings each inside a group of its own, where nothing at all is drawn between them
//!   and they still may not share a pass.

use std::sync::Arc;

use kurbo::Shape;

use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size, Vec2};

use smallvec::SmallVec;

use crate::clip::ClipLink;
use crate::group::GroupBoundary;
use crate::id::{ClipId, VectorId};
use crate::paint::PaintRef;
use crate::prim::Quad;
use crate::scene::Scene;
use crate::vector::VectorItem;

/// The surface the fixtures are laid out on.
pub(crate) fn viewport() -> Size<i32, Device> {
    Size::new(1920, 1080)
}

/// A device rectangle.
pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A rectangular path, which is all the geometry these fixtures need.
fn path(bounds: Rect<DevicePx, Device>) -> Arc<kurbo::BezPath> {
    Arc::new(
        kurbo::Rect::new(
            bounds.origin.x.0 as f64,
            bounds.origin.y.0 as f64,
            (bounds.origin.x.0 + bounds.size.width.0) as f64,
            (bounds.origin.y.0 + bounds.size.height.0) as f64,
        )
        .to_path(0.1),
    )
}

/// A grey fill, interned into the scene.
fn grey(scene: &mut Scene, level: f32) -> PaintRef {
    let id = scene.paints.solid(Color::srgb(level, level, level, 1.0));
    PaintRef::solid(id)
}

/// Pushes one vector item under `clip`.
fn vector(scene: &mut Scene, id: u32, bounds: Rect<DevicePx, Device>, clip: ClipId) {
    let fill = grey(scene, 0.6);
    scene.push_vector(VectorItem::filled(VectorId(id), path(bounds), fill).clipped(clip));
}

/// Twenty chart regions in a five-by-four grid, each clipped to its card.
///
/// With `legend_above`, each region's legend quad is drawn over its chart, so a composite covering
/// two regions would hide the first region's legend. That is the whole of rule 3.
pub(crate) fn dashboard(legend_above: bool) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());

    let backdrop = grey(&mut scene, 0.1);
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 1920.0, 1080.0), backdrop));

    for index in 0..20u32 {
        let column = (index % 5) as f32;
        let row = (index / 5) as f32;
        let x = 16.0 + column * 380.0;
        let y = 16.0 + row * 268.0;
        let card = rect(x, y, 360.0, 248.0);
        let plot = rect(x + 16.0, y + 40.0, 328.0, 192.0);
        let legend = rect(x + 240.0, y + 48.0, 96.0, 56.0);

        let card_fill = grey(&mut scene, 0.15);
        scene.push_quad(Quad::filled(card, card_fill));
        let legend_fill = grey(&mut scene, 0.25);
        if !legend_above {
            scene.push_quad(Quad::filled(legend, legend_fill));
        }
        let clip = scene.clips.only(ClipLink::rect(card));
        vector(&mut scene, index, plot, clip);
        if legend_above {
            scene.push_quad(Quad::filled(legend, legend_fill));
        }
    }
    scene
}

/// Twelve avatars in a row, each with its own rounded clip and nothing drawn between them.
pub(crate) fn avatars() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());

    let container = rect(16.0, 16.0, 640.0, 64.0);
    let fill = grey(&mut scene, 0.15);
    scene.push_quad(Quad::filled(container, fill));

    let row = scene.clips.only(ClipLink::rect(container));
    for index in 0..12u32 {
        let bounds = rect(24.0 + index as f32 * 48.0, 24.0, 48.0, 48.0);
        let clip = scene
            .clips
            .push(row, ClipLink::rounded(bounds, Vec2::splat(DevicePx(24.0))));
        vector(&mut scene, index, bounds, clip);
    }
    scene
}

/// A stacked-area chart: five bands, each overlapping the one below it, and nothing between them.
///
/// Rule 3 coalesces them into one pass, correctly — nothing non-vector lies between them. But a
/// per-item composite would blend the shared scratch twice over every overlap, so this is the
/// fixture that has to report that a per-item composite is not available.
pub(crate) fn stacked_area() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());

    let plot = rect(16.0, 16.0, 600.0, 400.0);
    let fill = grey(&mut scene, 0.15);
    scene.push_quad(Quad::filled(plot, fill));

    let clip = scene.clips.only(ClipLink::rect(plot));
    for band in 0..5u32 {
        // Each band starts a little lower and reaches the bottom, so consecutive bands overlap.
        let top = 40.0 + band as f32 * 40.0;
        vector(&mut scene, band, rect(32.0, top, 560.0, 400.0 - top), clip);
    }
    scene
}

/// Two cards side by side, the left one nesting its drawing three boxes deep and the right one
/// putting its drawing straight into the card.
///
/// The arrangement in which draw order **falls** as the frame is emitted. Order is allocated from
/// what a primitive overlaps, so each card restarts from just above the page beneath it: the left
/// card's drawing sits above three enclosing backgrounds and the right card's above one, and the
/// drawing emitted second therefore takes the lower order of the two. Nothing painted between the
/// two cards meets either drawing's ink, so rule 3 keeps them in one pass — and one pass is
/// composited by one draw, which is why where that draw goes has to be decided by the highest order
/// in the pass and not by the last one admitted.
pub(crate) fn falling_order() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());

    let page = grey(&mut scene, 0.1);
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 1920.0, 1080.0), page));

    // The left card, three nested backgrounds deep.
    for (depth, inset) in [0.0f32, 20.0, 40.0].into_iter().enumerate() {
        let fill = grey(&mut scene, 0.2 + depth as f32 * 0.05);
        scene.push_quad(Quad::filled(
            rect(
                100.0 + inset,
                100.0 + inset,
                300.0 - inset * 2.0,
                300.0 - inset * 2.0,
            ),
            fill,
        ));
    }
    vector(
        &mut scene,
        0,
        rect(160.0, 160.0, 180.0, 180.0),
        ClipId::ROOT,
    );

    // The right card, one background deep.
    let fill = grey(&mut scene, 0.2);
    scene.push_quad(Quad::filled(rect(700.0, 100.0, 300.0, 300.0), fill));
    vector(
        &mut scene,
        1,
        rect(760.0, 160.0, 180.0, 180.0),
        ClipId::ROOT,
    );

    scene
}

/// The two cards of [`falling_order`], with a badge painted over the second card's drawing after
/// both drawings have been emitted.
///
/// The arrangement rule 5 exists for. Rule 3 is consulted only when the next item arrives, so a
/// primitive painted after a pass's final item is recorded and never tested against it. The badge
/// overlaps the lower-ordered of the two drawings and so belongs above it, but one composite for
/// both drawings has to sit above the *higher*-ordered one as well — above the badge — and the
/// drawing it covers then shows through the badge.
pub(crate) fn badged_cards() -> Scene {
    let mut scene = falling_order();
    let badge = grey(&mut scene, 0.6);
    scene.push_quad(Quad::filled(rect(800.0, 200.0, 60.0, 60.0), badge));
    scene
}

/// A drawing outside a group, one inside it, and one outside again — none of the three near any
/// other.
///
/// The arrangement the occluder rules cannot see. A group's opening marker covers a region nothing
/// before it painted, so charging it as something painted in between splits nothing; only the fact
/// that it *changes which target is being drawn into* separates the first two drawings, and that is
/// not a question about coverage at all.
pub(crate) fn across_a_group() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());

    let backdrop = grey(&mut scene, 0.1);
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 1920.0, 1080.0), backdrop));

    vector(&mut scene, 0, rect(16.0, 16.0, 24.0, 24.0), ClipId::ROOT);

    let isolated = rect(200.0, 16.0, 100.0, 100.0);
    let start = GroupBoundary::start(isolated, 0.5, peniko::BlendMode::default(), SmallVec::new());
    scene.push_group(start.clone());
    vector(&mut scene, 1, rect(220.0, 40.0, 24.0, 24.0), ClipId::ROOT);
    scene.push_group(start.end());

    vector(&mut scene, 2, rect(500.0, 16.0, 24.0, 24.0), ClipId::ROOT);
    scene
}
