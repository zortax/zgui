//! A part of the desktop shell: what the contract asks for, in the shell's own terms.

use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerSurface,
};
use zgui_geom::{Css, CssPx, Edges, Size};
use zgui_platform::{ExclusiveZone, LayerPlacement};

/// The namespace a layer surface is made under when neither it nor the application names one.
pub const DEFAULT_NAMESPACE: &str = "zgui";

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
/// The shell spells the three answers as numbers: zero moves the surface out of the room other
/// surfaces reserve, a positive number reserves that much, and -1 extends the surface under the
/// room the others reserve. A reservation rounds to the nearest pixel and never goes below zero.
pub fn exclusive_zone(zone: ExclusiveZone) -> i32 {
    match zone {
        ExclusiveZone::Reserve(room) => room.0.round().max(0.0) as i32,
        ExclusiveZone::Ignore => -1,
        _ => 0,
    }
}

/// The extent to ask the shell for, in whole logical pixels.
///
/// An axis fastened to both of its edges is sent as zero, which is the shell's word for "as wide
/// as the edges are apart". The shell centres a surface that states an extent on such an axis,
/// at that extent. Every other axis is sent at least one pixel, because zero on an axis that is
/// not fastened to both edges is a protocol error.
pub fn size(size: Size<CssPx, Css>, anchors: Edges<bool>) -> (u32, u32) {
    let axis = |extent: CssPx, stretched: bool| {
        if stretched {
            0
        } else {
            extent.0.round().max(1.0) as u32
        }
    };
    (
        axis(size.width, anchors.left && anchors.right),
        axis(size.height, anchors.top && anchors.bottom),
    )
}

/// The namespace the surface is made under: its own, the application's, or the default.
///
/// An empty name counts as no name.
pub fn namespace<'a>(placement: &'a LayerPlacement, application: Option<&'a str>) -> &'a str {
    let named = |name: &&str| !name.is_empty();
    placement
        .namespace
        .as_deref()
        .filter(named)
        .or(application.filter(named))
        .unwrap_or(DEFAULT_NAMESPACE)
}

/// Applies `placement` and `extent` to `surface`, without committing it.
///
/// Everything the shell keeps about a layer surface goes out together: the shell forgets all of it
/// when the surface is unmapped, so the same requests are what a remap has to repeat.
pub fn apply(surface: &LayerSurface, placement: &LayerPlacement, extent: Size<CssPx, Css>) {
    surface.set_layer(layer(placement.layer));
    surface.set_anchor(anchor(placement.anchors));
    surface.set_keyboard_interactivity(keyboard(placement.keyboard));
    surface.set_exclusive_zone(exclusive_zone(placement.exclusive_zone));
    let margin = placement.margin.map(|side| side.0.round() as i32);
    surface.set_margin(margin.top, margin.right, margin.bottom, margin.left);
    let (width, height) = size(extent, placement.anchors);
    surface.set_size(width, height);
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_NAMESPACE, anchor, exclusive_zone, keyboard, layer, namespace, size};
    use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
    use zgui_geom::{CssPx, Edges, Size};
    use zgui_platform::{ExclusiveZone, LayerPlacement};

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
    fn avoiding_reserving_and_ignoring_are_three_different_numbers() {
        // 0 moves out of the room a bar reserves; -1 extends under it. A pop-over that sent -1
        // would open under the bar.
        assert_eq!(exclusive_zone(ExclusiveZone::Avoid), 0);
        assert_eq!(exclusive_zone(ExclusiveZone::Ignore), -1);
        assert_eq!(exclusive_zone(ExclusiveZone::Reserve(CssPx(36.4))), 36);
    }

    #[test]
    fn a_negative_reservation_is_clamped_rather_than_read_as_ignoring() {
        assert_eq!(exclusive_zone(ExclusiveZone::Reserve(CssPx(-8.0))), 0);
    }

    #[test]
    fn an_axis_fastened_to_both_edges_is_sent_as_zero() {
        let bar = Edges::new(true, true, false, true);
        assert_eq!(size(Size::new(CssPx(1920.0), CssPx(34.0)), bar), (0, 34));
        let corner = Edges::new(true, true, false, false);
        assert_eq!(size(Size::new(CssPx(320.4), CssPx(0.0)), corner), (320, 1));
        let full = Edges::new(true, true, true, true);
        assert_eq!(size(Size::new(CssPx(10.0), CssPx(10.0)), full), (0, 0));
    }

    #[test]
    fn the_namespace_falls_back_to_the_application_then_to_the_default() {
        let named = LayerPlacement::default().with_namespace("bar");
        assert_eq!(namespace(&named, Some("dev.example.Shell")), "bar");
        let unnamed = LayerPlacement::default();
        assert_eq!(
            namespace(&unnamed, Some("dev.example.Shell")),
            "dev.example.Shell"
        );
        assert_eq!(namespace(&unnamed, None), DEFAULT_NAMESPACE);
        assert_eq!(namespace(&unnamed, Some("")), DEFAULT_NAMESPACE);
        let empty = LayerPlacement::default().with_namespace("");
        assert_eq!(
            namespace(&empty, Some("dev.example.Shell")),
            "dev.example.Shell"
        );
        assert_eq!(namespace(&empty, None), DEFAULT_NAMESPACE);
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
