//! A `filter` or `backdrop-filter` chain, as the steps that execute it.

use zgui_scene::Filter;

use crate::filter::matrix::ColorMatrix;

/// One executable step of a filter chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// Every per-pixel filter of a run, folded into one map.
    Matrix(ColorMatrix),
    /// A gaussian blur of the given deviation in device pixels.
    Blur(f32),
    /// A blurred, displaced copy drawn behind the content.
    DropShadow {
        /// How far right the copy falls, in device pixels.
        offset_x: f32,
        /// How far down the copy falls, in device pixels.
        offset_y: f32,
        /// The deviation of the copy's blur, in device pixels.
        blur: f32,
        /// The copy's colour, premultiplied and gamma-encoded.
        color: [f32; 4],
    },
}

/// A filter chain, with consecutive per-pixel functions folded together.
///
/// Folding is exact rather than an optimisation to be justified: each per-pixel function is an
/// affine map on colour, and a run of them is the product of theirs. Folding is also all that is
/// possible — a blur does *not* commute with an affine map whose constant term is non-zero, so a
/// `contrast()` written before a `blur()` and one written after it are two different pictures, and
/// the steps stay in the order they were written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chain {
    /// The steps, in the order they apply.
    steps: Vec<Step>,
}

impl Chain {
    /// The chain for `filters`.
    pub fn of(filters: &[Filter]) -> Self {
        let mut steps: Vec<Step> = Vec::new();
        for filter in filters {
            match ColorMatrix::of(*filter) {
                Some(matrix) => match steps.last_mut() {
                    Some(Step::Matrix(held)) => *held = held.then(matrix),
                    _ => steps.push(Step::Matrix(matrix)),
                },
                None => steps.push(match filter {
                    Filter::Blur(deviation) => Step::Blur(deviation.max(0.0)),
                    Filter::DropShadow {
                        offset_x,
                        offset_y,
                        blur,
                        color,
                    } => Step::DropShadow {
                        offset_x: *offset_x,
                        offset_y: *offset_y,
                        blur: blur.max(0.0),
                        color: *color,
                    },
                    // Every remaining function has a matrix, so this arm is unreachable in the
                    // same sense that the two above are exhaustive.
                    _ => continue,
                }),
            }
        }
        steps.retain(|step| !matches!(step, Step::Matrix(matrix) if matrix.is_identity()));
        Self { steps }
    }

    /// Whether the chain does nothing at all.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The steps that need a pass of their own, and the map the final composite can carry.
    ///
    /// A chain ending in per-pixel functions costs nothing extra: the composite that was going to
    /// draw the group anyway applies the map as it samples. Only what precedes such a run needs a
    /// target written and read back.
    pub fn split(&self) -> (&[Step], ColorMatrix) {
        match self.steps.last() {
            Some(Step::Matrix(matrix)) => (&self.steps[..self.steps.len() - 1], *matrix),
            _ => (&self.steps, ColorMatrix::identity()),
        }
    }

    /// Every step, including one the composite could have carried.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

#[cfg(test)]
mod tests {
    use super::{Chain, Step};
    use crate::filter::matrix::ColorMatrix;
    use zgui_scene::Filter;

    #[test]
    fn a_run_of_per_pixel_functions_folds_into_one_map() {
        let chain = Chain::of(&[
            Filter::Saturate(0.4),
            Filter::Brightness(1.2),
            Filter::Invert(0.3),
        ]);
        assert_eq!(chain.steps().len(), 1);
        let color = [0.3, 0.6, 0.2, 1.0];
        let expected = ColorMatrix::invert(0.3)
            .apply(ColorMatrix::brightness(1.2).apply(ColorMatrix::saturate(0.4).apply(color)));
        let Step::Matrix(folded) = chain.steps()[0] else {
            panic!("a run of per-pixel functions is one map");
        };
        for (channel, wanted) in expected.iter().enumerate() {
            assert!((folded.apply(color)[channel] - wanted).abs() < 1e-5);
        }
    }

    #[test]
    fn a_blur_between_two_runs_keeps_them_apart() {
        let chain = Chain::of(&[
            Filter::Saturate(0.0),
            Filter::Blur(4.0),
            Filter::Brightness(2.0),
        ]);
        assert_eq!(chain.steps().len(), 3);
        assert_eq!(chain.steps()[1], Step::Blur(4.0));
        // The trailing run is free: the composite applies it while it samples.
        let (passes, folded) = chain.split();
        assert_eq!(passes.len(), 2);
        assert!(!folded.is_identity());
    }

    #[test]
    fn a_chain_of_only_per_pixel_functions_needs_no_pass_at_all() {
        let chain = Chain::of(&[Filter::Sepia(1.0)]);
        let (passes, folded) = chain.split();
        assert!(passes.is_empty());
        assert!(!folded.is_identity());
    }

    #[test]
    fn a_filter_that_changes_nothing_is_not_a_step() {
        assert!(Chain::of(&[]).is_empty());
        assert!(
            Chain::of(&[Filter::Saturate(1.0), Filter::Opacity(1.0)]).is_empty(),
            "an identity map is dropped rather than paid for"
        );
    }

    #[test]
    fn a_drop_shadow_keeps_its_displacement_and_its_colour() {
        let chain = Chain::of(&[Filter::DropShadow {
            offset_x: 2.0,
            offset_y: 3.0,
            blur: 5.0,
            color: [0.0, 0.0, 0.0, 0.5],
        }]);
        assert_eq!(
            chain.steps(),
            [Step::DropShadow {
                offset_x: 2.0,
                offset_y: 3.0,
                blur: 5.0,
                color: [0.0, 0.0, 0.0, 0.5],
            }]
        );
    }
}
