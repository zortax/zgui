//! Track sizing functions, converted one at a time and never as a list.
//!
//! Everything here is a pure function of one computed value, so a track list is walked lazily by an
//! iterator that allocates nothing and holds only a borrow of the style it came from.

use taffy::prelude::{TaffyAuto, TaffyFitContent, TaffyMaxContent, TaffyMinContent};
use taffy::{MaxTrackSizingFunction, MinTrackSizingFunction, TrackSizingFunction};
use zgui_css::values::grid::{TrackBreadthValue, TrackSizeValue};

/// One track's sizing function.
///
/// A bare `<flex>` means `minmax(auto, <flex>)`, which is why the single-breadth case is not simply
/// the same function twice.
pub fn track(size: &TrackSizeValue, scale: f32) -> TrackSizingFunction {
    match size {
        TrackSizeValue::Breadth(breadth) => TrackSizingFunction {
            min: min_breadth(breadth, scale),
            max: max_breadth(breadth, scale),
        },
        TrackSizeValue::Minmax(min, max) => TrackSizingFunction {
            min: min_breadth(min, scale),
            max: max_breadth(max, scale),
        },
        TrackSizeValue::FitContent(limit) => TrackSizingFunction {
            min: MinTrackSizingFunction::AUTO,
            max: fit_content(limit, scale),
        },
    }
}

/// A breadth in the minimum position, where a flex fraction is not allowed.
///
/// A flex fraction here is invalid CSS. It cannot arrive from the parser, and treating it as `auto`
/// rather than trusting it is what keeps a hand-built value from producing a track that grows
/// without bound in the position that is supposed to bound it.
fn min_breadth(breadth: &TrackBreadthValue, scale: f32) -> MinTrackSizingFunction {
    match breadth {
        TrackBreadthValue::Breadth(value) => match value.to_length() {
            Some(length) => MinTrackSizingFunction::length(length.px() * scale),
            None => match value.to_percentage() {
                Some(percentage) => MinTrackSizingFunction::percent(percentage.0),
                None => MinTrackSizingFunction::AUTO,
            },
        },
        TrackBreadthValue::MinContent => MinTrackSizingFunction::MIN_CONTENT,
        TrackBreadthValue::MaxContent => MinTrackSizingFunction::MAX_CONTENT,
        TrackBreadthValue::Auto | TrackBreadthValue::Flex(_) => MinTrackSizingFunction::AUTO,
    }
}

/// A breadth in the maximum position, where a flex fraction is the whole point.
fn max_breadth(breadth: &TrackBreadthValue, scale: f32) -> MaxTrackSizingFunction {
    match breadth {
        TrackBreadthValue::Breadth(value) => match value.to_length() {
            Some(length) => MaxTrackSizingFunction::length(length.px() * scale),
            None => match value.to_percentage() {
                Some(percentage) => MaxTrackSizingFunction::percent(percentage.0),
                None => MaxTrackSizingFunction::AUTO,
            },
        },
        TrackBreadthValue::MinContent => MaxTrackSizingFunction::MIN_CONTENT,
        TrackBreadthValue::MaxContent => MaxTrackSizingFunction::MAX_CONTENT,
        TrackBreadthValue::Auto => MaxTrackSizingFunction::AUTO,
        TrackBreadthValue::Flex(flex) => MaxTrackSizingFunction::fr(flex.0),
    }
}

/// The `fit-content()` limit, which is a length or a percentage and never a keyword.
fn fit_content(breadth: &TrackBreadthValue, scale: f32) -> MaxTrackSizingFunction {
    match breadth {
        TrackBreadthValue::Breadth(value) => match value.to_length() {
            Some(length) => MaxTrackSizingFunction::fit_content(taffy::LengthPercentage::length(
                length.px() * scale,
            )),
            None => match value.to_percentage() {
                Some(percentage) => MaxTrackSizingFunction::fit_content(
                    taffy::LengthPercentage::percent(percentage.0),
                ),
                None => MaxTrackSizingFunction::AUTO,
            },
        },
        _ => MaxTrackSizingFunction::AUTO,
    }
}

#[cfg(test)]
mod tests {
    use taffy::prelude::{TaffyAuto, TaffyFitContent, TaffyMaxContent, TaffyMinContent};
    use taffy::{MaxTrackSizingFunction, MinTrackSizingFunction};
    use zgui_css::values::grid::{TrackBreadthValue, TrackSizeValue};
    use zgui_css::values::length::{Length, LengthPercentage};

    use super::track;

    fn px(value: f32) -> TrackBreadthValue {
        TrackBreadthValue::Breadth(LengthPercentage::new_length(Length::new(value)))
    }

    #[test]
    fn a_length_track_is_the_same_length_at_both_ends_and_is_scaled() {
        let sizing = track(&TrackSizeValue::Breadth(px(10.0)), 2.0);
        assert_eq!(sizing.min, MinTrackSizingFunction::length(20.0));
        assert_eq!(sizing.max, MaxTrackSizingFunction::length(20.0));
    }

    #[test]
    fn a_bare_flex_fraction_is_a_minmax_of_auto_and_that_fraction() {
        let sizing = track(
            &TrackSizeValue::Breadth(TrackBreadthValue::Flex(zgui_css::values::grid::Flex(2.0))),
            1.0,
        );
        assert_eq!(sizing.min, MinTrackSizingFunction::AUTO);
        assert_eq!(sizing.max, MaxTrackSizingFunction::fr(2.0));
    }

    #[test]
    fn a_flex_fraction_in_the_minimum_position_is_refused_rather_than_carried() {
        let sizing = track(
            &TrackSizeValue::Minmax(
                TrackBreadthValue::Flex(zgui_css::values::grid::Flex(3.0)),
                px(50.0),
            ),
            1.0,
        );
        assert_eq!(sizing.min, MinTrackSizingFunction::AUTO);
        assert_eq!(sizing.max, MaxTrackSizingFunction::length(50.0));
    }

    #[test]
    fn fit_content_bounds_only_the_maximum() {
        let sizing = track(&TrackSizeValue::FitContent(px(30.0)), 1.0);
        assert_eq!(sizing.min, MinTrackSizingFunction::AUTO);
        assert_eq!(
            sizing.max,
            MaxTrackSizingFunction::fit_content(taffy::LengthPercentage::length(30.0))
        );
    }

    #[test]
    fn the_content_keywords_survive_both_positions() {
        let sizing = track(
            &TrackSizeValue::Minmax(TrackBreadthValue::MinContent, TrackBreadthValue::MaxContent),
            1.0,
        );
        assert_eq!(sizing.min, MinTrackSizingFunction::MIN_CONTENT);
        assert_eq!(sizing.max, MaxTrackSizingFunction::MAX_CONTENT);
    }
}
