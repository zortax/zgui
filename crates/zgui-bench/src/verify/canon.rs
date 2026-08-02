//! The one form of a display list two windows can be held against each other in.
//!
//! # What is being removed, and why it is not a relaxation
//!
//! A glyph is drawn through a slot in the texture atlas, and the slot is an *address*: it is decided
//! by the order in which glyphs were first rasterised, which is a fact about how long each window
//! has been running rather than about the picture either of them draws. The two windows a
//! differential compares rasterise in different orders by construction — the thorough one lays every
//! box out again before each turn and shapes text the incremental one never re-shapes — so their
//! atlases fill differently and the same glyph ends up at a different address in each.
//!
//! Two consequences reach the transcript, and neither is a pixel:
//!
//! * the address itself is printed beside every sprite;
//! * sprites are handed to the device grouped by the page they draw from, so a different assignment
//!   of glyphs to pages puts the sprites *of one painting order* in a different sequence.
//!
//! Both are undone here, and nothing else is. The address becomes an identity — the *n*-th distinct
//! tile of this list, numbered where it is first drawn from — so a window that started drawing two
//! different glyphs through one slot still differs from one that does not. The sequence is settled
//! only inside a run of sprites that share a painting order and a nesting depth, which is precisely
//! the span the atlas is free to permute; the painting order itself, and every primitive that is not
//! a sprite, is compared exactly where it was written.
//!
//! # The vector identity is the same kind of thing
//!
//! A vector item carries an id the rasteriser keeps its encoded geometry under. It is derived from
//! the arena slot of the fragment that emitted it, so like an atlas address it is decided by how the
//! window that drew it came to be rather than by what it draws — and the engine says so where it is
//! built: a collision "costs a re-encoding and never a wrong picture … an identity is a hint about
//! what is worth keeping rather than a promise about what a shape is". Two windows that have laid
//! out different numbers of times allocate different slots, so the same path is drawn under a
//! different id in each.
//!
//! It is renumbered exactly as a tile address is, and for exactly as much: the *n*-th distinct id of
//! this list, numbered where it first appears. What that keeps is the *pattern* — a window that drew
//! two different shapes under one id still differs from one that gave them an id each — while the
//! slot number itself, which is not a fact about the picture, stops being compared.
//!
//! What survives is every number, every paint, every clip, every transform, every primitive's kind
//! and its place in the painting order — for a vector item, its ink, its fill, its fill rule, its
//! stroke and its whole path data. A stale pixel is a difference in one of those.

/// The transcript in canonical form, ready to be compared line for line.
pub(crate) fn of(list: &str) -> String {
    let mut lines: Vec<String> = list.lines().map(ToOwned::to_owned).collect();
    settle_sprite_runs(&mut lines);
    renumber_tiles(&mut lines);
    renumber_vector_ids(&mut lines);
    let mut out = lines.join("\n");
    if list.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The three primitives drawn out of the atlas, which are the ones an address is printed for.
const SPRITES: [&str; 3] = ["mono_sprite", "subpixel_sprite", "color_sprite"];

/// What a sprite line may be permuted within: its nesting depth and its place in the painting order.
///
/// `None` for anything that is not a sprite, which is how a run is bounded — a quad between two
/// sprites of one order is a primitive the atlas has no say over, and the sprites either side of it
/// stay either side of it.
fn run_key(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let mut words = line.split_whitespace();
    let kind = words.next()?;
    if !SPRITES.contains(&kind) {
        return None;
    }
    let order = words.next()?.strip_prefix("order=")?;
    // The kind is deliberately not part of the key. Sprites of different kinds at one painting
    // order are batched together and split by page exactly as sprites of one kind are.
    Some((indent, order))
}

/// Sorts each maximal run of sprites sharing a painting order into one order.
///
/// By the line with its address masked, so the sequence a run is put into is a fact about the
/// picture. Two sprites of one order that agree on everything else are the same glyph drawn twice in
/// the same place, and the pair is indistinguishable whichever way round it is written.
fn settle_sprite_runs(lines: &mut [String]) {
    let mut start = 0;
    while start < lines.len() {
        let Some(key) = run_key(&lines[start]) else {
            start += 1;
            continue;
        };
        let mut end = start + 1;
        while end < lines.len() && run_key(&lines[end]) == Some(key) {
            end += 1;
        }
        lines[start..end].sort_by_key(|line| masked(line));
        start = end;
    }
}

/// A line split around the atlas address, when it carries one.
///
/// The address is two fields: which tile, and — for the coverage sprites — which rectangle of the
/// page it was rasterised into. The second is written `texels=rect(x, y, w, h)`, spaces and all, so
/// it ends at its bracket rather than at the next space.
struct Address<'a> {
    /// Everything before ` tile=`.
    before: &'a str,
    /// The two fields together, which is what identifies one rasterisation.
    whole: String,
    /// Everything after them.
    after: &'a str,
}

/// Splits `line` around its atlas address, or answers `None` when it draws out of no atlas.
fn address(line: &str) -> Option<Address<'_>> {
    let (before, rest) = line.split_once(" tile=")?;
    let end = rest.find(' ').unwrap_or(rest.len());
    let (tile, rest) = rest.split_at(end);
    let (texels, after) = match rest.strip_prefix(" texels=") {
        Some(texels) => {
            let end = texels.find(')').map_or(texels.len(), |at| at + 1);
            texels.split_at(end)
        }
        None => ("", rest),
    };
    Some(Address {
        before,
        whole: format!("{tile} {texels}"),
        after,
    })
}

/// The line with the atlas address taken out of it.
fn masked(line: &str) -> String {
    match address(line) {
        Some(found) => format!("{} tile=#{}", found.before, found.after),
        None => line.to_owned(),
    }
}

/// Replaces every atlas address with the position of its first appearance.
///
/// After the runs are settled, so the numbering is a function of the canonical list rather than of
/// the sequence the device happened to be handed.
fn renumber_tiles(lines: &mut [String]) {
    let mut seen: Vec<String> = Vec::new();
    for line in lines.iter_mut() {
        let Some(found) = address(line) else {
            continue;
        };
        let index = seen
            .iter()
            .position(|held| *held == found.whole)
            .unwrap_or_else(|| {
                seen.push(found.whole);
                seen.len() - 1
            });
        *line = format!("{} tile=#{index}{}", found.before, found.after);
    }
}

/// What a vector item's identity is written as, and what it is followed by.
const VECTOR_ID: &str = " id=#";

/// Splits `line` around its vector identity, or answers `None` when it carries none.
///
/// The identity runs to the next space, which is where `ink=` begins.
fn vector_id(line: &str) -> Option<(&str, &str, &str)> {
    let (before, rest) = line.split_once(VECTOR_ID)?;
    let end = rest.find(' ').unwrap_or(rest.len());
    let (id, after) = rest.split_at(end);
    Some((before, id, after))
}

/// Replaces every vector identity with the position of its first appearance.
///
/// The pattern of sharing is what is kept: two items written under one id stay written under one,
/// and two written under an id each stay written under an id each. Only the number is dropped.
fn renumber_vector_ids(lines: &mut [String]) {
    let mut seen: Vec<String> = Vec::new();
    for line in lines.iter_mut() {
        let Some((before, id, after)) = vector_id(line) else {
            continue;
        };
        let index = seen.iter().position(|held| held == id).unwrap_or_else(|| {
            seen.push(id.to_owned());
            seen.len() - 1
        });
        *line = format!("{before}{VECTOR_ID}{index}{after}");
    }
}

#[cfg(test)]
mod tests {
    use super::of;

    /// One vector line, spelled the way the transcript spells them.
    fn vector(id: u32, x: u32) -> String {
        format!(
            "    vector order=20 id=#{id} ink=rect({x}, 737, 16, 2) fill=solid oklch(0.556, 0, 0, 1) \
             fill_rule=NonZero d=\"M {x},737 L 256,737 L 256,739 L {x},739 Z\" \
             clip=[rect(0, 0, 1520, 880)]"
        )
    }

    #[test]
    fn one_path_drawn_under_two_arena_slots_is_one_canonical_list() {
        // Two windows, the same two paths in the same two places, emitted by fragments that landed
        // in different arena slots. This is the whole of what 55 of the standing faults at `s8`
        // were: every other field of every one of those lines was identical.
        let one = [vector(1_811_242_258, 240), vector(1_811_242_259, 300)].join("\n");
        let two = [vector(3_120_999_758, 240), vector(3_120_999_759, 300)].join("\n");
        assert_ne!(
            one, two,
            "the two lists differ before they are canonicalised"
        );
        assert_eq!(of(&one), of(&two), "and agree after");
    }

    #[test]
    fn a_path_that_moved_still_differs() {
        // The mutation that says the canonical form still has teeth: one path sixty pixels to the
        // right, under the same identity, must survive canonicalisation as a difference.
        let one = [vector(7, 240), vector(8, 300)].join("\n");
        let two = [vector(7, 240), vector(8, 360)].join("\n");
        assert_ne!(of(&one), of(&two));
    }

    #[test]
    fn two_shapes_sharing_one_identity_still_differ_from_two_that_do_not() {
        // What renumbering by first appearance keeps. A window that drew both paths under one id
        // and one that gave them an id each are not the same list, because the second is a cache
        // that told the rasteriser two different things about one name.
        let shared = [vector(7, 240), vector(7, 300)].join("\n");
        let apart = [vector(7, 240), vector(8, 300)].join("\n");
        assert_ne!(of(&shared), of(&apart));
    }

    /// One sprite line, spelled the way the transcript spells them.
    fn sprite(order: u32, x: u32, slot: u32, texels: u32) -> String {
        format!(
            "  mono_sprite order={order} bounds=rect({x}, 8, 7, 6) color=premul_srgb[0, 0, 0, 1] \
             tile=mono:0#{slot} texels=rect({texels}, 16, 7, 6) clip=[rect(0, 0, 100, 100)]"
        )
    }

    #[test]
    fn one_picture_drawn_through_two_atlases_is_one_canonical_list() {
        // Two windows, the same three glyphs of one painting order in the same three places, packed
        // into different slots and therefore handed to the device in a different sequence. This is
        // the whole of what 33 of the 68 standing faults were.
        let one = [sprite(9, 10, 4356, 896), sprite(9, 20, 4368, 900)].join("\n");
        let two = [sprite(9, 20, 7000, 100), sprite(9, 10, 7016, 128)].join("\n");
        assert_ne!(
            one, two,
            "the two lists differ before they are canonicalised"
        );
        assert_eq!(of(&one), of(&two), "and agree after");
    }

    #[test]
    fn a_sprite_that_moved_still_differs() {
        // The mutation that says the canonical form still has teeth: one glyph one pixel to the
        // right, packed identically, must survive canonicalisation as a difference.
        let one = [sprite(9, 10, 4356, 896), sprite(9, 20, 4368, 900)].join("\n");
        let two = [sprite(9, 11, 4356, 896), sprite(9, 20, 4368, 900)].join("\n");
        assert_ne!(of(&one), of(&two));
    }

    #[test]
    fn sprites_of_two_painting_orders_are_not_permuted_into_each_other() {
        let one = [sprite(9, 20, 1, 8), sprite(10, 10, 2, 16)].join("\n");
        let two = [sprite(10, 10, 2, 16), sprite(9, 20, 1, 8)].join("\n");
        assert_ne!(
            of(&one),
            of(&two),
            "the painting order is the order things are drawn in and is compared as written",
        );
    }

    #[test]
    fn a_quad_between_two_sprites_keeps_them_where_they_are() {
        let quad = "  quad order=9 bounds=rect(0, 0, 4, 4) fill=solid srgb(1, 1, 1, 1)";
        let one = [sprite(9, 20, 1, 8), quad.to_owned(), sprite(9, 10, 2, 16)].join("\n");
        let two = [sprite(9, 10, 2, 16), quad.to_owned(), sprite(9, 20, 1, 8)].join("\n");
        assert_ne!(
            of(&one),
            of(&two),
            "a run is bounded by anything the atlas has no say over",
        );
    }

    #[test]
    fn two_glyphs_that_share_one_slot_differ_from_two_that_do_not() {
        // Why the address becomes an identity rather than disappearing. Both lists draw the same
        // two glyphs in the same two places; in the first they come from one tile and in the second
        // from two, which is a difference about the atlas that is also a difference about the
        // picture.
        let shared = [sprite(9, 10, 4356, 896), sprite(9, 20, 4356, 896)].join("\n");
        let apart = [sprite(9, 10, 4356, 896), sprite(9, 20, 4368, 896)].join("\n");
        assert_ne!(of(&shared), of(&apart));
    }

    #[test]
    fn a_list_without_a_sprite_in_it_is_left_exactly_as_it_was() {
        let list = "primitives 2 batches=1\n  quad order=3 bounds=rect(0, 0, 4, 4)\n  quad \
                    order=2 bounds=rect(0, 0, 4, 4)\n";
        assert_eq!(of(list), list);
    }
}
