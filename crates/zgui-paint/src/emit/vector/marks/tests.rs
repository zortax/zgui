//! How recognised prims become a mark's payload.

use zgui_scene::MarkFlags;

use super::{dedupe, runs};
use crate::emit::vector::recognise::{BUTT, Decomposition, Orientation, ROUND, SQUARE};

/// A decomposition of `capsules`, each with its caps, stroked at width 2.
fn capsules(capsules: &[([f32; 4], u8, u8)]) -> Decomposition {
    Decomposition {
        discs: Vec::new(),
        boxes: Vec::new(),
        capsules: capsules.iter().map(|(capsule, _, _)| *capsule).collect(),
        caps: capsules
            .iter()
            .map(|(_, start, end)| start | (end << 2))
            .collect(),
        half_width: 1.0,
        ink: [0.0, 0.0, 40.0, 40.0],
        orientation: Orientation::Positive,
        count: capsules.len(),
        max_extent: 40.0,
    }
}

/// Whether a payload vertex is a run separator.
fn separator(vertex: [f32; 2]) -> bool {
    vertex[0].is_nan() && vertex[1].is_nan()
}

#[test]
fn round_joined_capsules_become_nan_separated_runs() {
    // One polyline of two segments with a round join, and a second polyline elsewhere.
    let found = capsules(&[
        ([0.0, 0.0, 10.0, 5.0], BUTT, ROUND),
        ([10.0, 5.0, 20.0, 0.0], ROUND, BUTT),
        ([30.0, 30.0, 35.0, 38.0], BUTT, BUTT),
    ]);
    let mut vertices = Vec::new();
    let flags = runs(&found, &mut vertices).expect("one cap at every outer end");
    assert_eq!(flags, MarkFlags::caps(MarkFlags::BUTT, MarkFlags::BUTT));
    let shape: Vec<Option<[f32; 2]>> = vertices
        .iter()
        .map(|vertex| (!separator(*vertex)).then_some(*vertex))
        .collect();
    assert_eq!(
        shape,
        vec![
            None,
            Some([0.0, 0.0]),
            Some([10.0, 5.0]),
            Some([20.0, 0.0]),
            None,
            Some([30.0, 30.0]),
            Some([35.0, 38.0]),
            None,
        ]
    );
}

#[test]
fn a_dot_is_a_run_of_two_equal_vertices() {
    let found = capsules(&[([4.0, 4.0, 4.0, 4.0], ROUND, ROUND)]);
    let mut vertices = Vec::new();
    let flags = runs(&found, &mut vertices).expect("round caps");
    assert_eq!(flags, MarkFlags::caps(MarkFlags::ROUND, MarkFlags::ROUND));
    assert_eq!(vertices.len(), 4);
    assert_eq!(vertices[1], vertices[2]);
}

#[test]
fn mixed_outer_caps_are_declined() {
    let found = capsules(&[
        ([0.0, 0.0, 10.0, 0.0], ROUND, ROUND),
        ([20.0, 0.0, 30.0, 0.0], SQUARE, SQUARE),
    ]);
    assert_eq!(runs(&found, &mut Vec::new()), None);
    // A square inner end starts a run of its own, and declines among round outer ends.
    let mitred = capsules(&[
        ([0.0, 0.0, 10.0, 0.0], ROUND, SQUARE),
        ([10.0, 0.0, 10.0, 10.0], SQUARE, ROUND),
    ]);
    assert_eq!(runs(&mitred, &mut Vec::new()), None);
}

#[test]
fn duplicate_discs_go_only_past_eightfold_overdraw() {
    let disc = [5.0, 5.0, 4.0, 0.0];
    // Two identical discs over a large ink: no dedupe.
    let mut sparse = vec![disc, disc];
    dedupe(&mut sparse, [0.0, 0.0, 100.0, 100.0]);
    assert_eq!(sparse.len(), 2);
    // Twenty of them over their own ink cover it about twelve times over, and collapse to one.
    let mut dense = vec![disc; 20];
    dense.push([5.0, 6.0, 4.0, 0.0]);
    dedupe(&mut dense, [1.0, 1.0, 9.0, 10.0]);
    assert_eq!(dense, vec![disc, [5.0, 6.0, 4.0, 0.0]]);
}
