//! Box alignment, from the one packed byte every alignment property computes to.
//!
//! Four keywords are degraded rather than carried: `self-start` and `self-end` differ from `start`
//! and `end` only under mixed writing modes, `last baseline` aligns like a first baseline, and
//! `anchor-center` centres. Each is recorded where it happens so that the loss is visible in the
//! code rather than in a document.

use taffy::{AlignContent, AlignContentKeyword, AlignItems, AlignItemsKeyword, AlignmentSafety};
use zgui_css::values::align::AlignFlags;

/// Where a container puts its items, or nothing to let the algorithm choose.
///
/// `left` and `right` resolve against the writing direction, so the caller passes it in.
pub fn align_items(flags: AlignFlags, rtl: bool) -> Option<AlignItems> {
    let safety = safety(flags);
    let keyword = match flags.value() {
        AlignFlags::AUTO | AlignFlags::NORMAL => return None,
        AlignFlags::START => AlignItemsKeyword::Start,
        AlignFlags::END => AlignItemsKeyword::End,
        AlignFlags::FLEX_START => AlignItemsKeyword::FlexStart,
        AlignFlags::FLEX_END => AlignItemsKeyword::FlexEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => AlignItemsKeyword::Center,
        AlignFlags::BASELINE | AlignFlags::LAST_BASELINE => AlignItemsKeyword::Baseline,
        AlignFlags::STRETCH => AlignItemsKeyword::Stretch,
        AlignFlags::SELF_START => AlignItemsKeyword::Start,
        AlignFlags::SELF_END => AlignItemsKeyword::End,
        AlignFlags::LEFT => {
            if rtl {
                AlignItemsKeyword::End
            } else {
                AlignItemsKeyword::Start
            }
        }
        AlignFlags::RIGHT => {
            if rtl {
                AlignItemsKeyword::Start
            } else {
                AlignItemsKeyword::End
            }
        }
        _ => return None,
    };
    Some(AlignItems { keyword, safety })
}

/// Where a container puts the whole block of its content, or nothing to let the algorithm choose.
///
/// The two baseline keywords are not valid here and fall back to `start`, which is what a
/// container does with a value it cannot honour.
pub fn align_content(flags: AlignFlags, rtl: bool) -> Option<AlignContent> {
    let safety = safety(flags);
    let keyword = match flags.value() {
        AlignFlags::AUTO | AlignFlags::NORMAL => return None,
        AlignFlags::START | AlignFlags::BASELINE | AlignFlags::LAST_BASELINE => {
            AlignContentKeyword::Start
        }
        AlignFlags::END => AlignContentKeyword::End,
        AlignFlags::FLEX_START => AlignContentKeyword::FlexStart,
        AlignFlags::FLEX_END => AlignContentKeyword::FlexEnd,
        AlignFlags::CENTER | AlignFlags::ANCHOR_CENTER => AlignContentKeyword::Center,
        AlignFlags::STRETCH => AlignContentKeyword::Stretch,
        AlignFlags::SPACE_BETWEEN => AlignContentKeyword::SpaceBetween,
        AlignFlags::SPACE_AROUND => AlignContentKeyword::SpaceAround,
        AlignFlags::SPACE_EVENLY => AlignContentKeyword::SpaceEvenly,
        AlignFlags::SELF_START => AlignContentKeyword::Start,
        AlignFlags::SELF_END => AlignContentKeyword::End,
        AlignFlags::LEFT => {
            if rtl {
                AlignContentKeyword::End
            } else {
                AlignContentKeyword::Start
            }
        }
        AlignFlags::RIGHT => {
            if rtl {
                AlignContentKeyword::Start
            } else {
                AlignContentKeyword::End
            }
        }
        _ => return None,
    };
    Some(AlignContent { keyword, safety })
}

/// `justify-items`, whose initial value is `legacy` and means "no preference".
pub fn justify_items(flags: AlignFlags, rtl: bool) -> Option<AlignItems> {
    if flags.contains(AlignFlags::LEGACY) {
        return None;
    }
    align_items(flags, rtl)
}

/// Whether an item that overflows is allowed to be pushed out of its container's start edge.
fn safety(flags: AlignFlags) -> AlignmentSafety {
    if flags.contains(AlignFlags::SAFE) {
        AlignmentSafety::Safe
    } else {
        AlignmentSafety::Unsafe
    }
}

#[cfg(test)]
mod tests {
    use taffy::{AlignContentKeyword, AlignItemsKeyword, AlignmentSafety};
    use zgui_css::values::align::AlignFlags;

    use super::{align_content, align_items, justify_items};

    #[test]
    fn no_preference_is_expressed_as_no_value_rather_than_as_a_default() {
        // A default here would override the algorithm's own choice, which differs between flex
        // and grid, and neither of them is `start`.
        assert_eq!(align_items(AlignFlags::NORMAL, false), None);
        assert_eq!(align_items(AlignFlags::AUTO, false), None);
        assert_eq!(align_content(AlignFlags::NORMAL, false), None);
    }

    #[test]
    fn the_physical_keywords_follow_the_writing_direction() {
        assert_eq!(
            align_items(AlignFlags::LEFT, false).map(|value| value.keyword),
            Some(AlignItemsKeyword::Start)
        );
        assert_eq!(
            align_items(AlignFlags::LEFT, true).map(|value| value.keyword),
            Some(AlignItemsKeyword::End)
        );
        assert_eq!(
            align_items(AlignFlags::RIGHT, true).map(|value| value.keyword),
            Some(AlignItemsKeyword::Start)
        );
    }

    #[test]
    fn a_modifier_does_not_hide_the_keyword_underneath_it() {
        let safe_centre = AlignFlags::CENTER | AlignFlags::SAFE;
        let value = align_items(safe_centre, false).expect("a keyword");
        assert_eq!(value.keyword, AlignItemsKeyword::Center);
        assert_eq!(value.safety, AlignmentSafety::Safe);
        let plain_centre = align_items(AlignFlags::CENTER, false).expect("a keyword");
        assert_eq!(plain_centre.safety, AlignmentSafety::Unsafe);
    }

    #[test]
    fn a_baseline_keyword_is_not_valid_for_content_and_falls_back_to_start() {
        assert_eq!(
            align_content(AlignFlags::BASELINE, false).map(|value| value.keyword),
            Some(AlignContentKeyword::Start)
        );
        assert_eq!(
            align_items(AlignFlags::BASELINE, false).map(|value| value.keyword),
            Some(AlignItemsKeyword::Baseline)
        );
    }

    #[test]
    fn legacy_justification_is_no_preference() {
        assert_eq!(
            justify_items(AlignFlags::LEGACY | AlignFlags::CENTER, false),
            None
        );
        assert!(justify_items(AlignFlags::CENTER, false).is_some());
    }

    #[test]
    fn the_distribution_keywords_only_exist_for_content() {
        assert_eq!(
            align_content(AlignFlags::SPACE_BETWEEN, false).map(|value| value.keyword),
            Some(AlignContentKeyword::SpaceBetween)
        );
        assert_eq!(align_items(AlignFlags::SPACE_BETWEEN, false), None);
    }
}
