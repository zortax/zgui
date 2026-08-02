//! The property every incremental path has to keep: laying out again produces the same layout.
//!
//! A layout that has been invalidated and recomputed must be indistinguishable from one computed
//! from nothing at the same size. Every cache in the crate — the shaped paragraphs, the atomic
//! inlines' nested layouts, the algorithms' own per-box caches — is a chance to answer a question
//! with a stale answer, and the failure is silent: a box a few pixels out, two frames after the
//! thing that moved it.

mod support;

use support::{Element, Fixture, lay_out, measurer};

/// A deterministic generator, so a failure names a tree that can be built again.
struct Rng(u64);

impl Rng {
    /// The next value.
    fn next(&mut self) -> u64 {
        // xorshift64*, which is enough for choosing between four shapes.
        let mut state = self.0;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.0 = state;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A value below `bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// The words a generated paragraph is made of.
const WORDS: [&str; 6] = [
    "alpha bravo",
    "delta gamma kappa",
    "sigma",
    "omega alpha bravo delta",
    "gamma kappa sigma omega",
    "alpha",
];

/// A random tree, three levels deep at most.
fn tree(rng: &mut Rng, depth: u32) -> Element {
    let name = match rng.below(3) {
        0 => "row",
        1 => "stack",
        _ => "para",
    };
    let mut element = Element::new(name);
    if name == "para" || depth == 0 {
        return element.text(WORDS[rng.below(WORDS.len() as u64) as usize]);
    }
    let mut children = Vec::new();
    for _ in 0..1 + rng.below(3) {
        children.push(tree(rng, depth - 1));
    }
    element = element.children(children);
    element
}

/// The stylesheet the generated names are laid out with.
const CSS: &str = "root { display: block }
     row { display: flex; gap: 4px }
     stack { display: block }
     para { display: block }
     span { display: inline }";

#[test]
fn incremental_layout_produces_the_same_result_as_a_fresh_one() {
    let mut rng = Rng(0x5eed_1234_9abc_def1);
    for case in 0..64 {
        let shape = tree(&mut rng, 3);
        let first = 120.0 + rng.below(600) as f32;
        let second = 120.0 + rng.below(600) as f32;

        // One store laid out at the first size, invalidated, and laid out again at the second.
        let fixture = Fixture::new(shape_of(&shape), CSS);
        let mut incremental = fixture.box_tree();
        let mut content = measurer();
        lay_out(&mut incremental, &mut content, first, 4000.0);
        let root = incremental.root().expect("a root");
        zgui_layout::tree::dirty::mark_dirty(&mut incremental, root);
        lay_out(&mut incremental, &mut content, second, 4000.0);

        // And one that only ever saw the second size.
        let fresh_fixture = Fixture::new(shape_of(&shape), CSS);
        let mut fresh = fresh_fixture.box_tree();
        let mut fresh_content = measurer();
        lay_out(&mut fresh, &mut fresh_content, second, 4000.0);

        assert_eq!(
            zgui_layout::tree::print::to_text(&incremental),
            zgui_layout::tree::print::to_text(&fresh),
            "case {case}: {first} then {second}",
        );
    }
}

/// Clones one generated tree, because a fixture consumes the element it is built from.
fn shape_of(element: &Element) -> Element {
    let mut copy = Element::new(element.name);
    if let Some(text) = element.text {
        copy = copy.text(text);
    }
    if !element.children.is_empty() {
        copy = copy.children(element.children.iter().map(shape_of).collect());
    }
    copy
}
