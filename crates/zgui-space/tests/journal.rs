//! The slots a spatial tree reports as changed since a version.

use zgui_geom::Matrix4;
use zgui_space::{ChangeCoverage, OwnSpace, PropertyOwner, SpatialId, SpatialTree};

fn owner(raw: u64) -> PropertyOwner {
    PropertyOwner::new(raw).expect("a handle is never the empty word")
}

fn slid(x: f32) -> Option<OwnSpace> {
    OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false)
}

/// A tree with `count` coordinate systems under the viewport.
fn cards(count: u64) -> (SpatialTree, Vec<SpatialId>) {
    let mut tree = SpatialTree::with_viewport();
    let viewport = tree.viewport();
    let cards = (0..count)
        .map(|at| tree.space_of(viewport, owner(at + 2), slid(at as f32)))
        .collect();
    (tree, cards)
}

fn changes(tree: &SpatialTree, version: zgui_space::TreeVersion) -> (ChangeCoverage, Vec<u32>) {
    let mut changed = Vec::new();
    let coverage = tree.changes_since(version, &mut changed);
    changed.sort_unstable();
    changed.dedup();
    (coverage, changed)
}

#[test]
fn only_the_slots_written_are_reported() {
    let (mut tree, cards) = cards(50);
    let version = tree.version();

    tree.place(cards[3], Matrix4::translation(30.0, 0.0, 0.0));
    tree.space_of(tree.viewport(), owner(9), slid(70.0));

    assert_eq!(
        changes(&tree, version),
        (
            ChangeCoverage::Delta,
            vec![cards[3].index(), cards[7].index()]
        )
    );
}

#[test]
fn rewriting_the_values_held_reports_nothing() {
    let (mut tree, cards) = cards(4);
    let version = tree.version();

    for (at, card) in cards.iter().enumerate() {
        tree.place(*card, Matrix4::translation(at as f32, 0.0, 0.0));
        tree.space_of(tree.viewport(), owner(at as u64 + 2), slid(at as f32));
    }

    assert_eq!(changes(&tree, version), (ChangeCoverage::Delta, vec![]));
    assert_eq!(tree.version(), version);
}

#[test]
fn a_released_slot_is_reported_when_released_and_when_the_frame_ends() {
    let (mut tree, cards) = cards(3);
    let version = tree.version();
    tree.release(owner(3));
    let (_, released) = changes(&tree, version);

    let ended = tree.version();
    tree.recycle();

    assert_eq!(released, vec![cards[1].index()]);
    assert_eq!(
        changes(&tree, ended),
        (ChangeCoverage::Delta, vec![cards[1].index()])
    );
}

#[test]
fn a_version_of_another_tree_asks_for_everything() {
    let (first, _) = cards(2);
    let (mut second, cards) = cards(2);
    second.place(cards[0], Matrix4::translation(5.0, 0.0, 0.0));

    assert_eq!(changes(&second, first.version()).0, ChangeCoverage::All);
}

#[test]
fn a_reader_past_the_history_asks_for_everything() {
    let (mut tree, cards) = cards(1);
    let version = tree.version();

    for step in 1..=5_000 {
        tree.place(cards[0], Matrix4::translation(step as f32, 0.0, 0.0));
    }

    assert_eq!(changes(&tree, version).0, ChangeCoverage::All);
    let recent = tree.version();
    tree.place(cards[0], Matrix4::translation(-1.0, 0.0, 0.0));
    assert_eq!(
        changes(&tree, recent),
        (ChangeCoverage::Delta, vec![cards[0].index()])
    );
}
