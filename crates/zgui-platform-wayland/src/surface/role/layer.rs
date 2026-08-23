//! A part of the desktop shell: what the contract asks for, in the shell's own terms.

use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerSurface,
};
use zgui_geom::CssPx;
use zgui_platform::LayerPlacement;

/// Which layer of the desktop, in the shell's numbering.
pub const fn layer(layer: zgui_platform::Layer) -> Layer {
    match layer {
        zgui_platform::Layer::Background => Layer::Background,
        zgui_platform::Layer::Bottom => Layer::Bottom,
        zgui_platform::Layer::Overlay => Layer::Overlay,
        _ => Layer::Top,
    }
}

/// Which edges the surface is fastened to.
pub fn anchor(anchors: zgui_geom::Edges<bool>) -> Anchor {
    let mut edges = Anchor::empty();
    edges.set(Anchor::TOP, anchors.top);
    edges.set(Anchor::RIGHT, anchors.right);
    edges.set(Anchor::BOTTOM, anchors.bottom);
    edges.set(Anchor::LEFT, anchors.left);
    edges
}

/// How much of the keyboard the surface takes.
pub const fn keyboard(
    interactivity: zgui_platform::KeyboardInteractivity,
) -> KeyboardInteractivity {
    match interactivity {
        zgui_platform::KeyboardInteractivity::Exclusive => KeyboardInteractivity::Exclusive,
        zgui_platform::KeyboardInteractivity::OnDemand => KeyboardInteractivity::OnDemand,
        _ => KeyboardInteractivity::None,
    }
}

/// How much room the compositor should reserve, in whole logical pixels.
///
/// Absent means "as much as the surface's own extent along its anchored edge", which the shell
/// spells as -1 and which is what a dock wants. Zero reserves nothing, which is what an overlay
/// wants, and the two must not be confused.
pub fn exclusive_zone(zone: Option<CssPx>) -> i32 {
    zone.map_or(-1, |zone| zone.0.round().max(0.0) as i32)
}

/// Applies `placement` to `surface`, without committing it.
pub fn apply(surface: &LayerSurface, placement: &LayerPlacement) {
    surface.set_layer(layer(placement.layer));
    surface.set_anchor(anchor(placement.anchors));
    surface.set_keyboard_interactivity(keyboard(placement.keyboard));
    surface.set_exclusive_zone(exclusive_zone(placement.exclusive_zone));
    let margin = placement.margin.map(|side| side.0.round() as i32);
    surface.set_margin(margin.top, margin.right, margin.bottom, margin.left);
}

#[cfg(test)]
mod tests {
    use super::{anchor, exclusive_zone, keyboard, layer};
    use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
    use zgui_geom::{CssPx, Edges};

    #[test]
    fn every_layer_of_the_desktop_has_one_in_the_shell() {
        assert_eq!(layer(zgui_platform::Layer::Background), Layer::Background);
        assert_eq!(layer(zgui_platform::Layer::Bottom), Layer::Bottom);
        assert_eq!(layer(zgui_platform::Layer::Top), Layer::Top);
        assert_eq!(layer(zgui_platform::Layer::Overlay), Layer::Overlay);
    }

    #[test]
    fn a_bar_across_the_top_is_anchored_to_three_edges() {
        let edges = anchor(Edges::new(true, true, false, true));
        assert_eq!(edges, Anchor::TOP | Anchor::RIGHT | Anchor::LEFT);
    }

    #[test]
    fn anchoring_to_nothing_is_a_centred_surface_rather_than_an_error() {
        assert_eq!(
            anchor(Edges::new(false, false, false, false)),
            Anchor::empty()
        );
    }

    #[test]
    fn reserving_nothing_and_reserving_the_surfaces_own_extent_are_different_answers() {
        // -1 is the shell's word for "as much as I am tall"; 0 is "none at all". A dock that sent
        // 0 would have windows opening underneath it.
        assert_eq!(exclusive_zone(None), -1);
        assert_eq!(exclusive_zone(Some(CssPx(0.0))), 0);
        assert_eq!(exclusive_zone(Some(CssPx(36.4))), 36);
    }

    #[test]
    fn a_negative_reservation_is_clamped_rather_than_read_as_the_default() {
        assert_eq!(exclusive_zone(Some(CssPx(-8.0))), 0);
    }

    #[test]
    fn a_surface_takes_no_keyboard_unless_it_asked() {
        assert_eq!(
            keyboard(zgui_platform::KeyboardInteractivity::None),
            KeyboardInteractivity::None
        );
        assert_eq!(
            keyboard(zgui_platform::KeyboardInteractivity::Exclusive),
            KeyboardInteractivity::Exclusive
        );
        assert_eq!(
            keyboard(zgui_platform::KeyboardInteractivity::OnDemand),
            KeyboardInteractivity::OnDemand
        );
    }
}
