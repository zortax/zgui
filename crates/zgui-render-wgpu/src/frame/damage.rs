//! Which rectangles a frame redraws.

use zgui_bits::DamageSet;
use zgui_geom::{Device, Rect};

/// The rectangles a frame redraws, in the composed target's device pixels.
///
/// A full set becomes the whole of `used`, and every rectangle is clipped to it. There is exactly
/// one place this is decided so that there is exactly one answer to "which pixels did this frame
/// promise to have redrawn" — the promise a `backdrop-filter` depends on.
///
/// **The rectangles only ever scissor the composed target.** An acquired surface texture is a
/// brand-new resource marked wholly uninitialised on every acquisition, so loading from one costs
/// a full clear before any of this frame's commands run; a partial copy onto it would come out
/// black everywhere it did not write. The copy to the surface is therefore unconditional and
/// covers all of it, and damage is a property of the target that outlives the frame.
pub fn rects(damage: &DamageSet, used: Rect<i32, Device>) -> Vec<Rect<i32, Device>> {
    if used.is_empty() {
        return Vec::new();
    }
    if damage.is_full() {
        return vec![used];
    }
    damage
        .rects()
        .iter()
        .filter_map(|rect| rect.intersection(used))
        .filter(|rect| !rect.is_empty())
        .collect()
}

/// How many device pixels a rectangle covers.
pub fn area(rect: Rect<i32, Device>) -> u64 {
    rect.size.width.max(0) as u64 * rect.size.height.max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::{area, rects};
    use zgui_bits::DamageSet;
    use zgui_geom::{Device, Point, Rect, Size};

    fn used() -> Rect<i32, Device> {
        Rect::new(Point::new(0, 0), Size::new(128, 128))
    }

    #[test]
    fn a_full_set_is_the_whole_of_what_the_surface_occupies() {
        assert_eq!(rects(&DamageSet::full(), used()), vec![used()]);
    }

    #[test]
    fn an_empty_set_redraws_nothing_at_all() {
        assert!(rects(&DamageSet::new(), used()).is_empty());
    }

    #[test]
    fn a_rectangle_reaching_past_the_surface_is_cut_to_it() {
        let mut damage = DamageSet::<4>::new();
        damage.absorb(Rect::new(Point::new(100, 100), Size::new(200, 200)));
        assert_eq!(
            rects(&damage, used()),
            vec![Rect::new(Point::new(100, 100), Size::new(28, 28))]
        );
    }

    #[test]
    fn a_rectangle_wholly_outside_the_surface_is_dropped_rather_than_clamped_to_nothing() {
        let mut damage = DamageSet::<4>::new();
        damage.absorb(Rect::new(Point::new(400, 400), Size::new(10, 10)));
        assert!(rects(&damage, used()).is_empty());
    }

    #[test]
    fn the_rectangles_stay_disjoint_so_no_pixel_is_redrawn_twice() {
        let mut damage = DamageSet::<4>::new();
        damage.absorb(Rect::new(Point::new(0, 0), Size::new(20, 20)));
        damage.absorb(Rect::new(Point::new(10, 10), Size::new(20, 20)));
        damage.absorb(Rect::new(Point::new(60, 60), Size::new(10, 10)));
        let planned = rects(&damage, used());
        assert_eq!(planned.len(), 2);
        assert_eq!(
            planned.iter().map(|rect| area(*rect)).sum::<u64>(),
            30 * 30 + 10 * 10
        );
    }
}
