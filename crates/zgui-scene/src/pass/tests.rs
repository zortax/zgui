//! The coalescing policy, measured on the fixtures it exists for.

use zgui_bits::DamageSet;
use zgui_geom::{Point, Rect, Size};

use crate::clip::ClipLink;
use crate::id::ClipId;
use crate::pass::fixture::{
    across_a_group, avatars, badged_cards, dashboard, falling_order, rect, stacked_area, viewport,
};
use crate::pass::overlap::Overlap;
use crate::pass::warning::PassWarning;
use crate::scene::Scene;

/// A damage set covering one card of the dashboard grid.
fn one_card() -> DamageSet {
    let mut damage = DamageSet::new();
    damage.absorb(Rect::new(Point::new(16, 16), Size::new(360, 248)));
    damage
}

/// A damage set covering three cards on three different rows.
fn three_cards() -> DamageSet {
    let mut damage = DamageSet::new();
    for index in [0usize, 7, 13] {
        let column = (index % 5) as i32;
        let row = (index / 5) as i32;
        damage.absorb(Rect::new(
            Point::new(16 + column * 380, 16 + row * 268),
            Size::new(360, 248),
        ));
    }
    damage
}

/// How many passes `scene` plans under `overlap` against `damage`.
fn passes(scene: &mut Scene, damage: &DamageSet, overlap: Overlap) -> usize {
    scene.finish_with(damage, overlap);
    scene.pass_plan().len()
}

/// **The measurement.** All three readings of rule 3 are implemented, and on the twenty-region
/// dashboard they cost twenty, four and one pass respectively.
///
/// The fixture is the one where the readings can disagree: each chart's own card background is
/// drawn immediately *below* it, so it is never something a composite could hide. The order-blind
/// reading charges it as intervening anyway and splits at every region; the ordering-aware
/// bounding box stops charging it but still splits once a row's accumulated box has grown across
/// the grid; and the per-item-ink reading charges nothing, because no card background ever meets an
/// earlier chart's own ink.
///
/// Keeping the two rejected readings as selectable policy is what makes the chosen one a
/// measurement rather than a claim, and it costs nothing at run time.
#[test]
fn the_three_readings_of_rule_three_cost_twenty_four_and_one_pass() {
    let full = DamageSet::full();

    assert_eq!(
        passes(&mut dashboard(false), &full, Overlap::BoundingBoxOrderBlind),
        20,
        "the order-blind bounding box charges every card's own background as intervening"
    );
    assert_eq!(
        passes(&mut dashboard(false), &full, Overlap::BoundingBox),
        4,
        "ordering-aware, but a bounding box of a grid is mostly empty space"
    );
    assert_eq!(
        passes(&mut dashboard(false), &full, Overlap::PerItemInk),
        1,
        "per-item ink: a card background never meets an earlier chart's own ink"
    );
}

/// Per-item ink is the policy, so the default has to be it and not something merely adjacent.
#[test]
fn the_default_reading_is_the_one_that_was_measured_cheapest() {
    assert_eq!(Overlap::default(), Overlap::PerItemInk);
    let mut scene = dashboard(false);
    scene.finish(&DamageSet::full());
    assert_eq!(scene.pass_plan().len(), 1);
}

/// The other half of the measurement: where z-order **genuinely** requires a pass per region, every
/// reading agrees and the tightest one saves nothing.
///
/// With each legend drawn over its chart, a composite spanning two regions would hide the first
/// region's legend. Twenty passes is then the correct answer and not a coalescing defect — which is
/// why the count is a statement about the content and never about the policy.
#[test]
fn a_legend_drawn_over_its_chart_costs_a_pass_per_region_under_every_reading() {
    let full = DamageSet::full();
    for reading in [
        Overlap::BoundingBoxOrderBlind,
        Overlap::BoundingBox,
        Overlap::PerItemInk,
    ] {
        assert_eq!(
            passes(&mut dashboard(true), &full, reading),
            20,
            "{reading:?}"
        );
    }
    // Dropping rule 3 entirely is what a per-item composite would cost, and the condition for it
    // holds here: no two charts overlap each other.
    assert_eq!(passes(&mut dashboard(true), &full, Overlap::Never), 1);
}

/// The pass count is the number of damaged interleaved regions, not a constant.
#[test]
fn the_pass_count_follows_the_damaged_regions() {
    let mut scene = dashboard(true);
    scene.finish(&one_card());
    assert_eq!(scene.pass_plan().len(), 1);
    assert_eq!(
        scene.pass_plan().culled,
        19,
        "rule 1 drops every item the damage set misses"
    );

    let mut scene = dashboard(true);
    scene.finish(&three_cards());
    assert_eq!(scene.pass_plan().len(), 3);
    assert_eq!(scene.pass_plan().culled, 17);
}

#[test]
fn a_legend_drawn_under_its_chart_never_forces_a_pass() {
    let mut scene = dashboard(false);
    scene.finish(&DamageSet::full());
    assert_eq!(
        scene.pass_plan().len(),
        1,
        "nothing is drawn over any chart, so nothing has to be split around"
    );
    assert_eq!(scene.pass_plan().clip_layers, 20, "one card clip per chart");
}

#[test]
fn an_avatar_row_costs_one_pass_and_absorbs_its_clips() {
    let mut scene = avatars();
    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();

    assert_eq!(plan.len(), 1);
    assert_eq!(plan.items.len(), 12);
    assert_eq!(
        plan.clip_layers, 12,
        "each avatar's own rounded clip becomes a layer rather than a pass"
    );
    let pass = &plan.passes[0];
    assert_ne!(
        pass.clip,
        ClipId::ROOT,
        "the row's container clip is shared"
    );
    for item in plan.items_of(pass) {
        assert_ne!(item.residual, ClipId::ROOT);
    }
}

/// **The soundness condition for a per-item composite**, on the one fixture that fails it.
///
/// Five stacked bands overlap each other and nothing is drawn between them, so rule 3 coalesces
/// them into one pass — correctly. Compositing them one at a time would blend the shared scratch
/// twice over each overlap, so the flag has to be false, and the pass is composited by one draw.
#[test]
fn instanced_is_false_when_two_items_overlap() {
    let mut scene = stacked_area();
    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();

    assert_eq!(
        plan.len(),
        1,
        "nothing intervenes, so the bands share a pass"
    );
    assert_eq!(plan.items.len(), 5);
    assert!(
        !plan.passes[0].instanced,
        "overlapping items must not be composited one at a time"
    );

    // And on the fixtures whose items do not overlap, the flag is set.
    let mut row = avatars();
    row.finish(&DamageSet::full());
    assert!(row.pass_plan().passes[0].instanced);

    let mut grid = dashboard(true);
    grid.finish(&DamageSet::full());
    assert!(grid.pass_plan().passes[0].instanced);
}

#[test]
fn a_frame_with_no_paths_plans_no_work_at_all() {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());
    let fill = {
        let id = scene
            .paints
            .solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
        crate::paint::PaintRef::solid(id)
    };
    for index in 0..61 {
        scene.push_quad(crate::prim::Quad::filled(
            rect(index as f32 * 4.0, 0.0, 3.0, 3.0),
            fill,
        ));
    }
    scene.finish(&DamageSet::full());
    assert!(scene.pass_plan().is_empty());
}

#[test]
fn an_undamaged_frame_plans_nothing_and_says_how_much_it_dropped() {
    let mut scene = dashboard(true);
    scene.finish(&DamageSet::new());
    assert!(scene.pass_plan().is_empty());
    assert_eq!(scene.pass_plan().culled, 20);
}

/// Rule 0: the one case where a clip costs a whole pass, and the one item kind that cannot share.
#[test]
fn a_clip_the_vector_scene_cannot_express_gets_a_pass_of_its_own() {
    use std::sync::Arc;

    use kurbo::Shape;

    use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};

    use crate::clip::MaskSource;
    use crate::id::VectorId;
    use crate::spatial::SpatialId;
    use crate::vector::VectorItem;

    let mut scene = Scene::new();
    scene.begin_frame(viewport());
    let fill = {
        let id = scene
            .paints
            .solid(zgui_color::Color::srgb(1.0, 1.0, 1.0, 1.0));
        crate::paint::PaintRef::solid(id)
    };
    let path = |bounds: Rect<zgui_geom::DevicePx, zgui_geom::Device>| {
        Arc::new(
            kurbo::Rect::new(
                bounds.origin.x.0 as f64,
                bounds.origin.y.0 as f64,
                (bounds.origin.x.0 + bounds.size.width.0) as f64,
                (bounds.origin.y.0 + bounds.size.height.0) as f64,
            )
            .to_path(0.1),
        )
    };

    let masked = scene.clips.only(ClipLink::Mask {
        tile: AtlasTile {
            texture: TextureId::new(TextureKind::Mono, 0),
            tile: TileId(1),
            bounds: Rect::new(Point::new(0, 0), Size::new(8, 8)),
        },
        transform: SpatialId::VIEWPORT,
        source: MaskSource::Raster,
    });

    // Three adjacent, non-overlapping items with nothing drawn between them: without rule 0 they
    // would all coalesce into one pass.
    scene.push_vector(VectorItem::filled(
        VectorId(0),
        path(rect(0.0, 0.0, 20.0, 20.0)),
        fill,
    ));
    scene.push_vector(
        VectorItem::filled(VectorId(1), path(rect(40.0, 0.0, 20.0, 20.0)), fill).clipped(masked),
    );
    scene.push_vector(VectorItem::filled(
        VectorId(2),
        path(rect(80.0, 0.0, 20.0, 20.0)),
        fill,
    ));

    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();
    assert_eq!(plan.len(), 3, "the masked item neither joins nor is joined");

    let masked_pass = plan
        .passes
        .iter()
        .find(|pass| pass.clip == masked)
        .expect("the masked item's pass binds its own clip");
    assert_eq!(plan.items_of(masked_pass).len(), 1);
    assert_eq!(plan.items_of(masked_pass)[0].residual, ClipId::ROOT);
}

#[test]
fn a_pass_region_is_tile_aligned_and_its_item_inks_are_relative_to_it() {
    let mut scene = avatars();
    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();
    let pass = &plan.passes[0];

    assert_eq!(pass.region.origin.x % 16, 0);
    assert_eq!(pass.region.origin.y % 16, 0);
    for item in plan.items_of(pass) {
        assert!(item.ink.origin.x >= 0 && item.ink.origin.y >= 0);
        assert!(item.ink.right() <= pass.region.size.width);
    }
}

#[test]
fn the_warning_fires_on_a_damaged_frame_and_stays_quiet_on_a_full_repaint() {
    let mut scene = dashboard(true);
    scene.finish(&DamageSet::full());
    let plan = scene.pass_plan();

    assert!(plan.len() >= PassWarning::THRESHOLD);
    assert!(
        plan.warning(true).is_none(),
        "a full repaint of n interleaved regions genuinely needs n passes"
    );
    let warning = plan.warning(false).expect("a damaged frame at 20 passes");
    assert_eq!(warning.passes, 20);
    assert!(warning.message().contains("20"));

    let mut quiet = dashboard(false);
    quiet.finish(&DamageSet::full());
    assert!(quiet.pass_plan().warning(false).is_none());
}

/// **A pass may not span a group boundary**, whatever is or is not painted across it.
///
/// A rasterisation pass is composited by a *single* draw, recorded where the pass's last item falls
/// in the painting order — and a group is a target of its own, entered and left at those markers,
/// scissored to the group's own region. A pass whose items straddle a boundary therefore has its
/// one composite recorded inside whichever group was open at the end, and everything on the other
/// side is scissored away: rasterised, counted, and not on the screen.
///
/// The fixture is the case the coverage rules cannot answer. The group's opening marker covers a
/// region nothing before it painted, so charging it as an intervening primitive splits nothing —
/// no reading of rule 3 separates the drawing before the group from the drawing inside it.
#[test]
fn a_pass_ends_where_the_target_does_however_little_is_painted_across_the_boundary() {
    let mut scene = across_a_group();
    let planned = passes(&mut scene, &DamageSet::full(), Overlap::PerItemInk);
    assert_eq!(
        planned, 3,
        "the drawing before the group, the one inside it and the one after it are three targets \
         and therefore three passes"
    );
    for pass in &scene.pass_plan().passes {
        assert_eq!(
            pass.items.len(),
            1,
            "a pass holding two of these holds items from two different targets"
        );
    }
}

/// **A pass composites above every item in it, not where its last item draws.**
///
/// Draw order is allocated from what a primitive overlaps, so it does not rise as a frame is
/// emitted: two panels side by side each restart from just above the page beneath them. A pass
/// spanning both therefore ends on an item whose order is *lower* than one it took in earlier, and
/// a composite placed there is drawn underneath the backgrounds the earlier drawing is nested
/// inside — which erases it. The composite is one draw for the whole pass, so the only order it can
/// take is the highest of them.
#[test]
fn a_composite_takes_the_highest_order_in_its_pass_and_not_the_last_one_admitted() {
    let mut scene = falling_order();
    assert_eq!(
        passes(&mut scene, &DamageSet::full(), Overlap::PerItemInk),
        1,
        "nothing painted between the two drawings meets either one's ink"
    );

    let plan = scene.pass_plan();
    let pass = &plan.passes[0];
    let orders: Vec<_> = plan.items[pass.items.clone()]
        .iter()
        .map(|planned| scene.primitives.vectors[planned.item].order)
        .collect();
    assert!(
        orders.last() < orders.iter().max(),
        "the fixture only measures anything while the last item admitted is not the highest, and \
         this frame's orders are {orders:?}"
    );
    assert_eq!(
        pass.composite_order,
        *orders.iter().max().expect("the pass holds items"),
        "a composite below any item of its own pass is a drawing the enclosing backgrounds erase"
    );
}

/// **A pass whose one composite cannot sit above every item is split into passes that can.**
///
/// Rule 3 keeps a primitive from being trapped under a composite while a pass is accumulating, but
/// it is only consulted when the next item arrives: a primitive painted after a pass's final item
/// is recorded and never tested. Here a badge is painted over the lower-ordered of two drawings
/// that share a pass. One composite for both would have to sit above the higher-ordered drawing,
/// and so above the badge, and the drawing under the badge would show through it. The items get one
/// pass each instead, each composited at its own order.
#[test]
fn a_pass_that_would_trap_a_primitive_under_its_composite_is_split_into_one_pass_per_item() {
    let mut plain = falling_order();
    assert_eq!(
        passes(&mut plain, &DamageSet::full(), Overlap::PerItemInk),
        1,
        "without the badge the two drawings share a pass"
    );

    let mut scene = badged_cards();
    assert_eq!(
        passes(&mut scene, &DamageSet::full(), Overlap::PerItemInk),
        2,
        "the badge painted over the lower drawing is what one composite cannot be placed around"
    );

    let plan = scene.pass_plan();
    for pass in &plan.passes {
        let orders: Vec<_> = plan.items[pass.items.clone()]
            .iter()
            .map(|planned| scene.primitives.vectors[planned.item].order)
            .collect();
        assert_eq!(
            pass.composite_order,
            *orders.iter().max().expect("the pass holds items"),
            "every composite still sits above every item of its own pass"
        );
    }
}
