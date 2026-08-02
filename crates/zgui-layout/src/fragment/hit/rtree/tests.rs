//! The tree against the answer a scan would have given.

use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::fragment::FragKey;
use crate::fragment::hit::rtree::home::Homes;

use super::{Placed, RTree};

/// One tree and the home table it files its entries in.
///
/// The table belongs to the forest in shipped code, because it is one fact per entry across every
/// tree. A test that exercises one tree still has to hold one, and holding it here keeps every case
/// below written in terms of the tree.
#[derive(Default)]
struct Tree {
    /// The hierarchy under test.
    tree: RTree,
    /// Where it filed what it holds.
    homes: Homes,
}

impl Tree {
    fn insert(&mut self, key: FragKey, bounds: Rect<DevicePx, Device>) {
        self.tree.insert(key, bounds, &mut self.homes);
    }

    fn remove(&mut self, key: FragKey) -> bool {
        self.tree.remove(key, &mut self.homes)
    }

    fn place(&mut self, key: FragKey, bounds: Rect<DevicePx, Device>) -> Placed {
        self.tree.place(key, bounds, &mut self.homes)
    }

    fn query(&self, point: Point<DevicePx, Device>, out: &mut Vec<FragKey>) {
        self.tree.query(point, out);
    }

    fn len(&self) -> usize {
        self.tree.len()
    }

    fn placements(&self) -> super::Placements {
        self.tree.placements()
    }
}

/// A fragment name for one slot number.
fn key(index: u32) -> FragKey {
    Key::new(
        index,
        Generation::FIRST,
        DomainId::new(DocumentId::FIRST, ArenaKind::new(2).expect("a valid arena")),
    )
}

/// A rectangle from four numbers.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A deterministic generator, so a failing case can be built again.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[test]
fn an_empty_tree_answers_nothing() {
    let tree = Tree::default();
    let mut found = Vec::new();
    tree.query(Point::new(DevicePx(0.0), DevicePx(0.0)), &mut found);
    assert!(found.is_empty());
    assert_eq!(tree.len(), 0);
}

#[test]
fn a_removed_entry_is_no_longer_found() {
    let mut tree = Tree::default();
    let bounds = rect(0.0, 0.0, 10.0, 10.0);
    tree.insert(key(1), bounds);
    let mut found = Vec::new();
    tree.query(Point::new(DevicePx(5.0), DevicePx(5.0)), &mut found);
    assert_eq!(found, vec![key(1)]);

    assert!(tree.remove(key(1)));
    found.clear();
    tree.query(Point::new(DevicePx(5.0), DevicePx(5.0)), &mut found);
    assert!(found.is_empty());
    assert_eq!(tree.len(), 0);
}

#[test]
fn the_tree_agrees_with_a_scan_over_ten_thousand_queries() {
    // The oracle is the definition of the answer: everything whose rectangle contains the point.
    // A hierarchy that dismisses a subtree it should have descended fails here and nowhere else.
    let mut rng = Rng(0x1234_5678_9abc_def0);
    let mut tree = Tree::default();
    let mut entries: Vec<(FragKey, Rect<DevicePx, Device>)> = Vec::new();
    for index in 0..400u32 {
        let bounds = rect(
            rng.below(900) as f32,
            rng.below(900) as f32,
            1.0 + rng.below(120) as f32,
            1.0 + rng.below(120) as f32,
        );
        tree.insert(key(index + 1), bounds);
        entries.push((key(index + 1), bounds));
    }
    // Half of them move, which is what an animation does one entry at a time.
    for index in (0..entries.len()).step_by(2) {
        let (name, bounds) = entries[index];
        let _ = bounds;
        assert!(tree.remove(name), "entry {index} was inserted");
        let moved = rect(
            rng.below(900) as f32,
            rng.below(900) as f32,
            1.0 + rng.below(120) as f32,
            1.0 + rng.below(120) as f32,
        );
        tree.insert(name, moved);
        entries[index] = (name, moved);
    }
    assert_eq!(tree.len(), entries.len());

    let mut found = Vec::new();
    for _ in 0..10_000 {
        let point = Point::new(
            DevicePx(rng.below(1000) as f32),
            DevicePx(rng.below(1000) as f32),
        );
        found.clear();
        tree.query(point, &mut found);
        found.sort_unstable();
        let mut expected: Vec<FragKey> = entries
            .iter()
            .filter(|(_, bounds)| bounds.contains(point))
            .map(|(name, _)| *name)
            .collect();
        expected.sort_unstable();
        assert_eq!(found, expected, "at {point:?}");
    }
}

#[test]
fn a_scrolled_document_moves_its_entries_without_reinserting_them() {
    // The shape of a scroll: every rectangle shifts by the same vector, every frame, and nothing
    // changes size. What is asserted is the *work*, and beside it that the answers are still the
    // ones a scan gives — a hierarchy that skipped the work and left an envelope too small would
    // pass the first assertion and fail the second.
    let mut tree = Tree::default();
    let mut entries: Vec<(FragKey, Rect<DevicePx, Device>)> = Vec::new();
    for index in 0..400u32 {
        let bounds = rect(
            f64::from(index % 20).mul_add(48.0, 8.0) as f32,
            f64::from(index / 20).mul_add(36.0, 8.0) as f32,
            40.0,
            28.0,
        );
        tree.insert(key(index + 1), bounds);
        entries.push((key(index + 1), bounds));
    }
    for _ in 0..8 {
        for entry in &mut entries {
            entry.1.origin.y = DevicePx(entry.1.origin.y.0 - 3.0);
            tree.place(entry.0, entry.1);
        }
    }
    let placed = tree.placements();
    let kept = placed.kept();
    assert_eq!(kept + placed.reinserted, 3_200, "every entry was placed");
    // Not all of them: the whole set is drifting one way, so the entries along the leading edge
    // really do leave the region the tree covers and really do have to be searched back in. What
    // is asserted is that the interior — which is nearly all of it — does not.
    assert!(
        placed.reinserted * 4 < kept,
        "a glide keeps entries among their neighbours: {placed:?}"
    );

    let mut rng = Rng(0x0f0f_0f0f_0f0f_0f0f);
    let mut found = Vec::new();
    for _ in 0..10_000 {
        let point = Point::new(
            DevicePx(rng.below(1000) as f32),
            DevicePx(rng.below(1000) as f32),
        );
        found.clear();
        tree.query(point, &mut found);
        found.sort_unstable();
        let mut expected: Vec<FragKey> = entries
            .iter()
            .filter(|(_, bounds)| bounds.contains(point))
            .map(|(name, _)| *name)
            .collect();
        expected.sort_unstable();
        assert_eq!(found, expected, "at {point:?}");
    }
    assert_eq!(tree.len(), entries.len());
}

#[test]
fn placing_an_entry_far_away_still_finds_it_there_and_not_where_it_was() {
    // The other half of `place`: a rectangle that has left its leaf's envelope has to be taken out
    // and put back, and an implementation that only rewrote it in the leaf would answer hits at
    // both ends of the move.
    let mut tree = Tree::default();
    for index in 0..64u32 {
        tree.insert(key(index + 1), rect(index as f32 * 10.0, 0.0, 8.0, 8.0));
    }
    tree.place(key(1), rect(5000.0, 5000.0, 8.0, 8.0));

    let mut found = Vec::new();
    tree.query(Point::new(DevicePx(2.0), DevicePx(2.0)), &mut found);
    assert!(found.is_empty(), "it is no longer where it was");
    found.clear();
    tree.query(Point::new(DevicePx(5002.0), DevicePx(5002.0)), &mut found);
    assert_eq!(found, vec![key(1)], "it is where it went");
    assert_eq!(tree.len(), 64);
}

#[test]
fn every_entry_is_reachable_by_name_after_a_thousand_placements() {
    // The home index and the hierarchy are two records of one fact. They drift silently: an entry
    // filed under a leaf that no longer holds it can never be removed again, so its rectangle
    // answers hits for ever at a place nothing is drawn.
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    let mut tree = Tree::default();
    let mut entries: Vec<(FragKey, Rect<DevicePx, Device>)> = Vec::new();
    for index in 0..300u32 {
        let bounds = rect(
            rng.below(700) as f32,
            rng.below(700) as f32,
            1.0 + rng.below(60) as f32,
            1.0 + rng.below(60) as f32,
        );
        tree.insert(key(index + 1), bounds);
        entries.push((key(index + 1), bounds));
    }
    for _ in 0..1_000 {
        let which = rng.below(entries.len() as u64) as usize;
        let moved = rect(
            rng.below(700) as f32,
            rng.below(700) as f32,
            1.0 + rng.below(60) as f32,
            1.0 + rng.below(60) as f32,
        );
        tree.place(entries[which].0, moved);
        entries[which].1 = moved;
    }
    for (name, _) in &entries {
        assert!(tree.remove(*name), "{name:?} was still reachable by name");
    }
    assert_eq!(tree.len(), 0);
    let mut found = Vec::new();
    tree.query(Point::new(DevicePx(350.0), DevicePx(350.0)), &mut found);
    assert!(found.is_empty());
}
