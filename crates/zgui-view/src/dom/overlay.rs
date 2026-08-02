//! Which overlay surface a portalled view lands on.

/// The stacking band an overlay belongs to.
///
/// One overlay root holding everything would order overlays by mount order, so a toast raised
/// before a dialog would paint beneath it. Naming the band instead puts the ordering where a style
/// sheet can see it, which is also where an application can override it.
///
/// The bands are ordered, and the order is the paint order.
///
/// ```
/// use zgui_view::OverlayLayer;
///
/// assert!(OverlayLayer::Toast > OverlayLayer::Modal);
/// assert!(OverlayLayer::Modal > OverlayLayer::Popover);
/// assert!(OverlayLayer::Popover > OverlayLayer::Content);
/// assert_eq!(OverlayLayer::default(), OverlayLayer::Popover);
/// assert_eq!(OverlayLayer::Modal.name(), "modal");
/// ```
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[non_exhaustive]
pub enum OverlayLayer {
    /// Content that is portalled but belongs with the page: a sticky region, an inline surface.
    Content,
    /// Popovers, menus, tooltips and dropdowns.
    #[default]
    Popover,
    /// Dialogs, sheets and drawers, which take the interaction over.
    Modal,
    /// Toasts and other notices, which sit above everything including a dialog.
    Toast,
}

impl OverlayLayer {
    /// Every band, in paint order.
    pub const ALL: &'static [Self] = &[Self::Content, Self::Popover, Self::Modal, Self::Toast];

    /// The name this band is written with, which is also the attribute value a style sheet
    /// selects on.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Popover => "popover",
            Self::Modal => "modal",
            Self::Toast => "toast",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OverlayLayer;

    #[test]
    fn the_declared_order_is_the_paint_order() {
        let mut sorted = OverlayLayer::ALL.to_vec();
        sorted.sort();
        assert_eq!(sorted, OverlayLayer::ALL);
    }

    #[test]
    fn every_band_has_a_distinct_name() {
        let mut names: Vec<&str> = OverlayLayer::ALL.iter().map(|layer| layer.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), OverlayLayer::ALL.len());
    }
}
