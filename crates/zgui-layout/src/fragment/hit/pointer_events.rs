//! Whether a fragment takes pointer events.
//!
//! `pointer-events: none` is not "invisible to the eye": the fragment is still painted, still has
//! geometry and is still in the accessibility tree. It simply stops being an answer to "what is
//! under the pointer", so the thing behind it answers instead — which is what makes an overlay that
//! must not swallow clicks possible at all.

use zgui_css::ComputedStyle;

/// Whether a fragment can be hit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PointerEvents {
    /// The fragment answers hit tests, which is the initial value.
    #[default]
    Auto,
    /// It does not, and whatever is behind it answers instead.
    None,
}

impl PointerEvents {
    /// Whether a fragment carrying this can be hit.
    pub const fn is_hittable(self) -> bool {
        matches!(self, Self::Auto)
    }
}

/// What one computed style says about being hit.
///
/// The property is inherited, so a child of a `pointer-events: none` element is unhittable through
/// its own computed value and not through a walk up the tree — which is what lets the index test one
/// entry at a time with nothing else to consult.
pub fn of(style: &ComputedStyle) -> PointerEvents {
    match style.get_inherited_ui().pointer_events {
        zgui_css::values::ui::PointerEventsValue::Auto => PointerEvents::Auto,
        _ => PointerEvents::None,
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::{PointerEvents, of};

    #[test]
    fn the_initial_value_takes_events() {
        let style = StyleDraft::initial().build();
        assert_eq!(of(&style), PointerEvents::Auto);
        assert!(PointerEvents::Auto.is_hittable());
        assert!(!PointerEvents::None.is_hittable());
    }
}
