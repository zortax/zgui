//! Coordinate systems mirrored as compositor transform and scroll nodes.

mod support;

use support::compositor::Recorded;
use support::{Element, Harness};
use zgui_geom::Matrix4;
use zgui_scene::spatial::{PropertyOwner, SpatialId};

/// Twenty transformed cards, each in a coordinate system of its own.
fn cards() -> Harness {
    let cards = (0..20).map(|_| Element::new("card")).collect();
    Harness::sized(
        Element::new("root").children(cards),
        "root { display: block; width: 400px; height: 400px }
         card { display: block; width: 10px; height: 10px; background: blue;
                transform: translateX(4px) }",
        400.0,
        400.0,
    )
}

/// The coordinate system the box of each element named `name` established.
fn spaces_of(harness: &Harness, name: &str) -> Vec<SpatialId> {
    harness
        .elements(name)
        .into_iter()
        .map(|element| {
            let key = harness.document.store().key_of(element);
            let box_ = harness.store.boxes_of(key)[0];
            harness
                .scene
                .spatial
                .of(PropertyOwner::of(box_))
                .expect("a transformed box establishes a coordinate system")
        })
        .collect()
}

#[test]
fn moving_coordinate_systems_writes_one_node_for_each() {
    let Some(mut output) = Recorded::new([400, 400]) else {
        return;
    };
    let mut harness = cards();
    output.frame_everything(&mut harness);
    let spaces = spaces_of(&harness, "card");

    let mut writes = Vec::new();
    for moved in [1, 5, 0] {
        for (at, space) in spaces.iter().take(moved).enumerate() {
            harness
                .scene
                .spatial
                .place(*space, Matrix4::translation(30.0 + at as f32, 0.0, 0.0));
        }
        let written = output.frame(&mut harness);
        assert_eq!(written.report.recorded, 0, "{:?}", written.report);
        writes.push(written.commit.property_writes);
    }

    // The first card moves in both frames, to a place it is already at in the second.
    assert_eq!(writes, vec![1, 4, 0]);
}

#[test]
fn a_moved_coordinate_system_moves_the_ones_below_it_without_writing_them() {
    let Some(mut output) = Recorded::new([400, 400]) else {
        return;
    };
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("outer").children(vec![Element::new("inner")]),
        ]),
        "root { display: block; width: 400px; height: 400px }
         outer { display: block; width: 100px; height: 100px; transform: translateX(10px) }
         inner { display: block; width: 20px; height: 20px; background: red;
                 transform: translateY(10px) }",
        400.0,
        400.0,
    );
    output.frame_everything(&mut harness);
    let outer = spaces_of(&harness, "outer")[0];

    harness
        .scene
        .spatial
        .place(outer, Matrix4::translation(110.0, 0.0, 0.0));
    let written = output.frame(&mut harness);

    assert_eq!(written.commit.property_writes, 1);
    let drawing = output.output.drawing().unwrap_or_default();
    if !drawing.is_empty() {
        assert!(
            drawing.contains("ink=rect(110, 10, 20, 20)"),
            "the inner card is drawn 100 pixels right of where it was:\n{drawing}"
        );
    }
}
