//! `overflow`, and the one value the layout algorithms have no representation for.

use zgui_css::values::size::OverflowValue;

/// How overflowing content affects layout.
///
/// `auto` has no direct answer: whether it reserves a scrollbar gutter depends on whether the
/// content overflows, which is not known while the size that decides it is being computed. It
/// therefore enters layout as `hidden`, which reserves nothing, and a container that turns out to
/// overflow is laid out a second time with the gutter reserved.
pub fn overflow(value: OverflowValue) -> taffy::Overflow {
    match value {
        OverflowValue::Visible => taffy::Overflow::Visible,
        OverflowValue::Clip => taffy::Overflow::Clip,
        OverflowValue::Hidden | OverflowValue::Auto => taffy::Overflow::Hidden,
        OverflowValue::Scroll => taffy::Overflow::Scroll,
    }
}

/// Whether a value needs the second pass that decides its gutter.
pub fn is_undecided(value: OverflowValue) -> bool {
    value == OverflowValue::Auto
}

/// Which of a box's axes need that second pass.
///
/// The sole definition of "is an undecided-overflow box", called both by the roster that decides
/// which boxes the gutter fixpoint looks at and by the fixpoint itself. A box the roster misses is
/// a scrollport whose gutter is never revised, which shows up as content that overflows a container
/// with no scrollbar in it — see [`axes_of`](crate::intrinsic::keywords::axes_of) for the same
/// argument made at greater length.
pub fn undecided_axes(style: &zgui_css::ComputedStyle) -> (bool, bool) {
    let box_ = style.get_box();
    (is_undecided(box_.overflow_x), is_undecided(box_.overflow_y))
}

/// The same, once layout has decided whether this axis reserves a gutter.
///
/// The decision is layout's rather than the style's in two cases and both go through here: an
/// `auto` box whose content was found to overflow, and a scroll container that has been locked and
/// keeps the gutter it had. A box that reserves a gutter is `scroll` to the layout algorithms
/// whatever its style says, because reserving the space *is* what that value means to them.
pub fn decided(value: OverflowValue, reserves_gutter: bool) -> taffy::Overflow {
    if reserves_gutter {
        return taffy::Overflow::Scroll;
    }
    overflow(value)
}

#[cfg(test)]
mod tests {
    use zgui_css::values::size::OverflowValue;

    use super::{is_undecided, overflow};

    #[test]
    fn auto_enters_layout_reserving_nothing_and_is_marked_undecided() {
        assert_eq!(overflow(OverflowValue::Auto), taffy::Overflow::Hidden);
        assert!(is_undecided(OverflowValue::Auto));
        // Everything else is decided by the value alone, which is what makes the second pass
        // reachable only for the one value that needs it.
        for value in [
            OverflowValue::Visible,
            OverflowValue::Clip,
            OverflowValue::Hidden,
            OverflowValue::Scroll,
        ] {
            assert!(!is_undecided(value), "{value:?}");
        }
    }

    #[test]
    fn scroll_is_the_only_value_a_gutter_is_reserved_for() {
        // A gutter is reserved for `scroll` and for nothing else, so `auto` must not arrive as
        // `scroll` before the content is known to overflow — it would reserve a scrollbar's width
        // in every container that turns out not to need one.
        assert_eq!(overflow(OverflowValue::Scroll), taffy::Overflow::Scroll);
        for value in [
            OverflowValue::Visible,
            OverflowValue::Clip,
            OverflowValue::Hidden,
            OverflowValue::Auto,
        ] {
            assert_ne!(overflow(value), taffy::Overflow::Scroll, "{value:?}");
        }
    }

    #[test]
    fn the_three_values_that_clip_are_kept_apart() {
        assert_eq!(overflow(OverflowValue::Visible), taffy::Overflow::Visible);
        assert_eq!(overflow(OverflowValue::Clip), taffy::Overflow::Clip);
        assert_eq!(overflow(OverflowValue::Hidden), taffy::Overflow::Hidden);
    }
}
