//! `aspect-ratio`, and the natural ratio it may defer to.

use zgui_css::values::size::{AspectRatioValue, PreferredRatio};

/// The ratio of width to height a box should keep, if any.
///
/// The property carries two things: an explicit ratio, and whether the box should prefer its
/// content's natural ratio when it has one. The layout algorithms take a single number, so the
/// natural ratio is supplied by the caller and preferred exactly when the property says `auto` —
/// which is what makes an image with `aspect-ratio: auto 2/1` keep its own proportions and fall
/// back to two-to-one only when it has none.
///
/// A degenerate ratio — one with a zero on either side — is discarded, because CSS says such a
/// ratio behaves as though none had been written.
pub fn aspect_ratio(value: &AspectRatioValue, natural: Option<f32>) -> Option<f32> {
    let (explicit, auto) = split(value);
    if auto {
        natural.or(explicit)
    } else {
        explicit.or(natural)
    }
}

/// The property's two halves: the written ratio, if it is not degenerate, and whether the
/// content's natural proportions are preferred.
///
/// Split out so a lowering can hold the style's half and supply a box's natural ratio later.
pub(crate) fn split(value: &AspectRatioValue) -> (Option<f32>, bool) {
    let explicit = match &value.ratio {
        PreferredRatio::None => None,
        PreferredRatio::Ratio(ratio) => {
            let (width, height) = ((ratio.0).0, (ratio.1).0);
            (width > 0.0 && height > 0.0).then_some(width / height)
        }
    };
    (explicit, value.auto)
}

#[cfg(test)]
mod tests {
    use zgui_css::values::size::{AspectRatioValue, PreferredRatio, RatioValue};

    use super::aspect_ratio;

    /// `auto <ratio>`: prefer the content's own proportions, fall back to this.
    fn auto_with(width: f32, height: f32) -> AspectRatioValue {
        AspectRatioValue {
            auto: true,
            ratio: PreferredRatio::Ratio(RatioValue::new(width, height)),
        }
    }

    /// A written ratio with no `auto`.
    fn fixed(width: f32, height: f32) -> AspectRatioValue {
        AspectRatioValue {
            auto: false,
            ratio: PreferredRatio::Ratio(RatioValue::new(width, height)),
        }
    }

    #[test]
    fn a_box_with_neither_a_ratio_nor_natural_proportions_keeps_none() {
        assert_eq!(aspect_ratio(&AspectRatioValue::auto(), None), None);
    }

    #[test]
    fn auto_prefers_the_content_and_a_written_ratio_overrides_it() {
        assert_eq!(aspect_ratio(&auto_with(2.0, 1.0), Some(1.5)), Some(1.5));
        assert_eq!(aspect_ratio(&auto_with(2.0, 1.0), None), Some(2.0));
        assert_eq!(aspect_ratio(&fixed(2.0, 1.0), Some(1.5)), Some(2.0));
    }

    #[test]
    fn a_degenerate_ratio_behaves_as_though_it_had_not_been_written() {
        assert_eq!(aspect_ratio(&fixed(0.0, 1.0), None), None);
        assert_eq!(aspect_ratio(&fixed(1.0, 0.0), Some(3.0)), Some(3.0));
    }
}
