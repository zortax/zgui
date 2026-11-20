//! The one form of a drawing two windows can be held against each other in.
//!
//! The drawing is reduced to what its picture depends on: every instance and every object drawn one
//! at a time, the nesting of groups, and the order of instances that overlap (see [`order`]). How
//! the compositor batched the instances, which opaque covers it found and where it binned marks are
//! its own plan, and they are left out.
//!
//! Two facts reach an instance that are not the picture, and both are undone (see [`line`]):
//!
//! - An atlas address. A glyph or an image is drawn from a tile of the compositor's atlas, and the
//!   tile is decided by the order in which the compositor first rasterised or uploaded each one.
//!   The two windows a differential compares have separate compositors that fill their atlases in
//!   different orders, so the same glyph is drawn from a different tile in each. The address
//!   becomes an identity: the *n*-th distinct tile of the canonical drawing. A window that draws
//!   two different glyphs from one tile still differs from one that does not.
//! - The paint space of an instance. A solid paint is the same colour at every point, so its origin
//!   and its brush change no pixel, and they are left out. A ramp on a path is written in the
//!   space its brush maps onto the path, so a brush that only translates is applied to the ramp's
//!   points and left out. A chunk moved by an offset carries its paint space along, and a chunk
//!   recorded again at the new place starts from where it is recorded.
//!
//! Every other number, paint, clip, transform and kind is compared exactly where the transcript
//! wrote it.

mod line;
mod order;

use crate::verify::canon::order::{Item, Scope};

/// Returns the drawing in canonical form, ready to be compared line for line.
pub(crate) fn of(drawing: &str) -> String {
    let mut open: Vec<(String, Scope)> = Vec::new();
    let mut top = Scope::default();
    for raw in drawing.lines() {
        if raw.starts_with("  group_begin ") {
            let (text, _) = line::canonical(raw);
            open.push((text, Scope::default()));
            continue;
        }
        if raw.starts_with("  group_end ") {
            let Some((text, inner)) = open.pop() else {
                continue;
            };
            let scope = open.last_mut().map_or(&mut top, |(_, scope)| scope);
            scope.items.push(Item::Group { text, inner });
            continue;
        }
        if !drawn(raw) {
            continue;
        }
        let (text, tile) = line::canonical(raw);
        let scope = open.last_mut().map_or(&mut top, |(_, scope)| scope);
        scope.items.push(Item::Line { text, tile });
    }
    let mut out = String::new();
    let mut tiles = Vec::new();
    top.write(&mut out, &mut tiles);
    out
}

/// The objects other than groups that a frame draws one at a time.
const OBJECTS: [&str; 2] = ["  backdrop ", "  external "];

/// Reports whether `line` is an instance or an object the frame draws.
fn drawn(line: &str) -> bool {
    let instance = line.starts_with("    ") && !line.starts_with("     ");
    instance || OBJECTS.iter().any(|object| line.starts_with(object))
}

#[cfg(test)]
mod tests {
    use super::of;

    fn glyph(x: u32, page: u32, slot: u32) -> String {
        format!(
            "    mono bounds=rect({x}, 8, 7, 6) paint=solid premul[0, 0, 0, 1] \
             tile=page{page}[{slot}, 0, 7, 6] clip=[rect(0, 0, 100, 100)]"
        )
    }

    fn quad(x: u32) -> String {
        format!("    quad bounds=rect({x}, 0, 20, 20) fill=solid premul[1, 1, 1, 1]")
    }

    #[test]
    fn one_picture_drawn_through_two_atlases_is_one_canonical_drawing() {
        let one = [glyph(10, 0, 4), glyph(20, 0, 12), glyph(30, 0, 4)].join("\n");
        let two = [glyph(10, 1, 70), glyph(20, 0, 3), glyph(30, 1, 70)].join("\n");
        assert_ne!(
            one, two,
            "the two drawings differ before they are canonicalised"
        );
        assert_eq!(of(&one), of(&two), "and agree after");
    }

    #[test]
    fn two_glyphs_sharing_one_tile_still_differ_from_two_that_do_not() {
        let shared = [glyph(10, 0, 4), glyph(20, 0, 4)].join("\n");
        let apart = [glyph(10, 0, 4), glyph(20, 0, 12)].join("\n");
        assert_ne!(of(&shared), of(&apart));
    }

    #[test]
    fn the_batches_a_drawing_was_drawn_in_are_left_out() {
        let one = format!(
            "batches 1\n  mono page0 0..2 ink=rect(0, 0, 9, 9)\n{}\n",
            glyph(10, 0, 4)
        );
        let two = format!(
            "batches 2\n  mono page3 0..1 ink=rect(0, 0, 9, 9)\n{}\ncover rect(0, 0, 4, 4) \
             batch=1 at=0\n",
            glyph(10, 3, 9)
        );
        assert_eq!(of(&one), of(&two));
    }

    #[test]
    fn apart_instances_may_come_in_either_order_and_overlapping_ones_may_not() {
        let apart = [quad(0), quad(50)];
        assert_eq!(
            of(&apart.join("\n")),
            of(&[apart[1].clone(), apart[0].clone()].join("\n"))
        );
        let overlapping = [quad(0), quad(10)];
        assert_ne!(
            of(&overlapping.join("\n")),
            of(&[overlapping[1].clone(), overlapping[0].clone()].join("\n"))
        );
    }

    #[test]
    fn an_instance_inside_a_group_differs_from_the_same_instance_outside_it() {
        let inside = format!(
            "  group_begin 0 bounds=rect(0, 0, 9, 9) opacity=0.5\n{}\n  group_end 0\n{}",
            quad(0),
            quad(50)
        );
        let outside = format!(
            "  group_begin 0 bounds=rect(0, 0, 9, 9) opacity=0.5\n{}\n  group_end 0\n{}",
            quad(50),
            quad(0)
        );
        assert_ne!(of(&inside), of(&outside));
    }

    #[test]
    fn the_origin_of_a_solid_paint_is_left_out_and_the_origin_of_a_ramp_is_kept() {
        let solid = "    quad bounds=rect(0, 16, 6, 21) fill=solid premul[0, 0, 0, 0.349] \
                     paint_origin=[0, 16] clip=[rect(0, 0, 100, 100)]";
        let unmoved = "    quad bounds=rect(0, 16, 6, 21) fill=solid premul[0, 0, 0, 0.349] \
                       clip=[rect(0, 0, 100, 100)]";
        assert_eq!(of(solid), of(unmoved));

        let ramp = "    quad bounds=rect(0, 16, 6, 21) fill=linear from=(0, 0) to=(1, 1) \
                    paint_origin=[0, 16] clip=[rect(0, 0, 100, 100)]";
        assert_eq!(of(ramp), format!("{ramp}\n"));
    }

    #[test]
    fn a_ramp_in_a_translated_paint_space_is_the_ramp_on_the_path() {
        let path = |from: u32, brush: i64| {
            format!(
                "    path ink=rect(66, 360, 51, 62) fill=linear from=(62, {from}) to=(428.5, {from}) \
                 in srgb rule=nonzero transform=[1, 0, 0, 1, 62, 421] brush=[1, 0, 0, 1, -62, {brush}] \
                 d=\"M 0,0 Z\""
            )
        };
        assert_eq!(of(&path(1676, -1708)), of(&path(1892, -1924)));
        assert_ne!(of(&path(1676, -1708)), of(&path(1676, -1924)));
    }
}
