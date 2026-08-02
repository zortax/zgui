//! How far the content actually moves for a delta that has nowhere to go.

/// How far past its end a container may be dragged, in device pixels.
///
/// The band is finite, so pulling harder stops moving anything rather than dragging the content off
/// the screen. Below the resistance curve it is what makes the edge feel like a limit rather than
/// like more content.
pub(crate) const BAND: f32 = 120.0;

/// The displacement reached by adding `added` to a container already displaced by `held`.
///
/// The displacement is carried as the *un*-resisted distance pulled so far and mapped through
/// `band * x / (band + x)` on the way out, so that adding to it is addition rather than an inverse
/// of the curve — and so that pulling and releasing repeatedly does not accumulate rounding.
pub(crate) fn resist(held: f32, added: f32) -> f32 {
    let pulled = unresist(held) + added;
    let magnitude = pulled.abs();
    pulled.signum() * (BAND * magnitude / (BAND + magnitude))
}

/// The distance that was pulled to reach a displacement of `held`.
fn unresist(held: f32) -> f32 {
    let magnitude = held.abs().min(BAND - 0.001);
    held.signum() * (BAND * magnitude / (BAND - magnitude))
}

#[cfg(test)]
mod tests {
    use super::{BAND, resist};

    #[test]
    fn the_band_is_never_left_however_hard_it_is_pulled() {
        let mut held = 0.0;
        for _ in 0..200 {
            held = resist(held, 50.0);
        }
        assert!(
            held < BAND,
            "ten thousand pixels of gesture dragged the content {held} past its end"
        );
    }

    #[test]
    fn the_first_pixels_move_almost_one_for_one_and_the_later_ones_do_not() {
        let early = resist(0.0, 2.0);
        let late = resist(80.0, 2.0) - 80.0;
        assert!(early > 1.9, "the edge does not feel stuck: {early}");
        assert!(late < early / 2.0, "and it stiffens as it goes: {late}");
    }

    #[test]
    fn a_pull_in_the_other_direction_displaces_the_other_way() {
        let up = resist(0.0, -40.0);
        assert!(up < 0.0);
        assert!(resist(up, 40.0).abs() < 1e-3);
    }
}
