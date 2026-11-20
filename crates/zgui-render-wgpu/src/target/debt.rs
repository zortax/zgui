//! What each texture of a rotated set still owes, so that a copy into it covers only that.
//!
//! A frame is copied into one texture of a set: a display's scanout buffers, or the images of a
//! swap chain this crate owns. Each of those textures keeps what it held, so the copy has to cover
//! what this frame wrote **and** what every frame wrote while this texture was not the one being
//! written. Without that, a texture shows on alternate frames whatever it held two frames ago.

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};

/// How many disjoint rectangles a texture's outstanding copy is tracked as.
///
/// Stated as a multiple of [`MAX_DAMAGE`](zgui_bits::MAX_DAMAGE) rather than as a number, because
/// below it the set cannot hold even one frame's damage: the rectangles merge, the merged ones
/// cover the gaps between them, and the copy writes pixels no frame ever changed. On a display
/// whose buffers are on the far side of a bus that is the largest thing a frame spends — measured
/// at half as much again as the frame's own damage, for a set one rectangle short of holding it.
///
/// Twice, because a texture is written every second or third frame and is owed what was drawn
/// while it was not. Frame to frame those rectangles mostly land on each other and absorb, so the
/// second copy is headroom rather than a count of anything.
///
/// The other end of the trade is a scissor and a draw per rectangle, inside a pass that is open
/// anyway. A draw costs about seventeen microseconds on the slowest driver this was measured
/// against, and a rectangle merged away wastes far more than that in pixels.
pub(crate) const STALE: usize = zgui_bits::MAX_DAMAGE * 2;

/// The outstanding copy of every texture of one set.
#[derive(Clone, Debug)]
pub(crate) struct Debt {
    /// Per texture: what was written into the composed target since this texture was last copied
    /// into.
    ///
    /// A [`DamageSet`] rather than a list, because it is the same problem the damage set already
    /// solves: keep a bounded number of disjoint rectangles and merge the cheapest pair when one
    /// more arrives.
    per_texture: Vec<DamageSet<STALE>>,
    /// What was written into the composed target since the last copy into any texture.
    ///
    /// The whole debt of a set whose caller reads each texture out and throws it away.
    uncopied: DamageSet<STALE>,
}

impl Debt {
    /// The debt of a new set of `count` textures, each of which holds nothing and owes everything.
    pub(crate) fn new(count: usize) -> Self {
        Self {
            per_texture: vec![DamageSet::full(); count],
            uncopied: DamageSet::full(),
        }
    }

    /// Records rectangles a frame wrote into the composed target.
    ///
    /// Called when the frame is composed, and not when it is copied out. A frame can be composed
    /// and never copied: the platform held it, or no texture was free. What it wrote is then owed
    /// with the next copy, whichever frame makes it.
    pub(crate) fn note_written(&mut self, rects: &[Rect<i32, Device>]) {
        for rect in rects {
            self.uncopied.absorb(*rect);
            for held in &mut self.per_texture {
                held.absorb(*rect);
            }
        }
    }

    /// Makes every texture owe everything, for a set whose contents are no longer known.
    #[cfg_attr(
        all(not(any(vulkan_hal, windows)), not(test)),
        expect(dead_code, reason = "only a native swap chain forgets its contents")
    )]
    pub(crate) fn forget(&mut self) {
        for held in &mut self.per_texture {
            *held = DamageSet::full();
        }
        self.uncopied = DamageSet::full();
    }

    /// Takes the rectangles the copy into `slot` has to cover, clipped to `size`.
    ///
    /// A rotated texture is owed everything written since it was last copied into. A `consumed`
    /// one is owed everything written since the last copy into any of them. Taking the debt
    /// clears it, because the copy that follows writes exactly those rectangles.
    pub(crate) fn take(
        &mut self,
        slot: usize,
        size: Size<i32, Device>,
        consumed: bool,
    ) -> Vec<Rect<i32, Device>> {
        let whole = Rect::new(Point::new(0, 0), size);
        let uncopied = core::mem::take(&mut self.uncopied);
        let owed = if consumed {
            uncopied
        } else {
            match self.per_texture.get_mut(slot) {
                Some(held) => core::mem::take(held),
                None => DamageSet::full(),
            }
        };
        if owed.is_full() {
            return vec![whole];
        }
        owed.rects()
            .iter()
            .filter_map(|rect| rect.intersection(whole))
            .filter(|rect| !rect.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Point, Rect, Size};

    use super::Debt;

    fn square(x: i32) -> Rect<i32, zgui_geom::Device> {
        Rect::new(Point::new(x, 0), Size::new(4, 4))
    }

    #[test]
    fn a_new_texture_owes_everything_once() {
        let size = Size::new(16, 16);
        let mut debt = Debt::new(2);
        assert_eq!(
            debt.take(0, size, false),
            [Rect::new(Point::new(0, 0), size)]
        );
        assert!(debt.take(0, size, false).is_empty());
    }

    #[test]
    fn a_texture_owes_what_was_written_while_it_was_not_copied_into() {
        let size = Size::new(16, 16);
        let mut debt = Debt::new(2);
        debt.take(0, size, false);
        debt.take(1, size, false);
        debt.note_written(&[square(0)]);
        assert_eq!(debt.take(0, size, false), [square(0)]);
        debt.note_written(&[square(8)]);
        let owed = debt.take(1, size, false);
        assert!(
            owed.iter().any(|rect| rect.contains_rect(square(0))),
            "{owed:?}"
        );
        assert!(
            owed.iter().any(|rect| rect.contains_rect(square(8))),
            "{owed:?}"
        );
    }

    #[test]
    fn forgetting_makes_every_texture_owe_everything() {
        let size = Size::new(16, 16);
        let mut debt = Debt::new(2);
        debt.take(0, size, false);
        debt.forget();
        assert_eq!(
            debt.take(0, size, false),
            [Rect::new(Point::new(0, 0), size)]
        );
    }
}
