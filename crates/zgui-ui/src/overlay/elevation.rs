//! Which band a surface goes on once the surface it was opened from is taken into account.
//!
//! # The model
//!
//! Every floating surface is portalled onto one of four ordered bands, and the band is chosen by
//! what kind of surface it is: a menu and a popover go on the popover band, a dialog and a sheet on
//! the modal band, a toast above both. That is the right answer for surfaces opened from the page,
//! and the wrong one the moment a surface is opened from *inside* another: a select inside a dialog
//! asks for the popover band, which is beneath the modal band the dialog is on, so its list is
//! drawn under the panel that opened it and the control is unusable.
//!
//! So the band a surface asks for is a **floor**, not an answer. A surface published to everything
//! inside it the band it is itself on; a surface opened inside another goes on the higher of the
//! band it asked for and the band it was opened from. Nesting to any depth follows, because the
//! surface that elevated itself publishes the band it ended up on rather than the one it asked for.
//!
//! A surface that asks for a band *above* the one it was opened from keeps it: a toast raised from
//! inside a dialog is still a toast, and the floor never pulls anything down.
//!
//! # Why the band is not enough on its own
//!
//! Landing on the same band as the surface that opened it puts the select's list beside the
//! dialog's panel, not over it. Within a band the order is the order the boxes are in, and that
//! order is the *reverse* of what is wanted: a portal written inside another portal's content is
//! mounted while that content is being built, so the inner surface reaches the band first and is
//! painted first — underneath.
//!
//! So the depth is carried as well as the band. Each surface counts one deeper than the one it was
//! opened from and publishes that depth to its own content as `--zui-overlay-depth`, which the
//! shared sheet reads as the `z-index` of the box the band stacks — the scrim, the positioner and a
//! modal surface's own panel. Ordering is then stated rather than inherited from mount order, and
//! it nests to any depth because the number does.

use zgui::prelude::*;
use zgui::{component, view};

use crate::overlay::SHEET;
use crate::overlay::style::OverlayStyle;

/// Where the surfaces opened inside this one sit: which band, and how far up it.
///
/// Published by every floating surface for its own content, and read by every floating surface
/// written inside one. A surface with none above it reads nothing, keeps the band it asked for and
/// sits at the bottom of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SurfaceElevation {
    /// The band, which no surface opened inside this one may go below.
    band: OverlayLayer,
    /// How many surfaces deep this one is, which is its `z-index` on that band.
    depth: usize,
}

impl SurfaceElevation {
    /// The elevation the calling scope is inside, when it is inside a floating surface at all.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The band this elevation puts a surface on.
    #[must_use]
    pub fn layer(self) -> OverlayLayer {
        self.band
    }

    /// How far up that band it sits, which is the `z-index` its own boxes take.
    #[must_use]
    pub fn depth(self) -> usize {
        self.depth
    }

    /// Where a surface asking for `wanted` actually goes, here.
    ///
    /// The band is the higher of the two, so that a select inside a dialog rises to the dialog's
    /// band while a toast raised from the same dialog stays on its own, above it. The depth is one
    /// more than whatever opened it, whichever band that was, so that the surface opened last is
    /// the one on top.
    #[must_use]
    pub fn raise(wanted: OverlayLayer) -> Self {
        match Self::current() {
            Some(under) => Self {
                band: wanted.max(under.band),
                depth: under.depth.saturating_add(1),
            },
            None => Self {
                band: wanted,
                depth: 0,
            },
        }
    }

    /// Publishes this elevation as what everything built below the calling scope is opened from.
    ///
    /// Called by a surface with the elevation it *ended up at*, which is what makes the model nest:
    /// a popover that rose to the modal band raises the menu opened inside it to the modal band
    /// too, and one step further up it.
    pub fn publish(self) {
        provide_local_context(self);
    }
}

/// Carries one surface's depth to the boxes its band stacks.
///
/// It generates no box of its own — the band's own rules are written against what a portal puts
/// there, and a real box between them would be a second layout to get out of the way. What it does
/// is inherit `--zui-overlay-depth` to the scrim, the positioner and the panel below it, which is
/// where the shared sheet reads it as a `z-index`.
#[component]
pub fn Elevated(
    /// Where the surface inside this ended up.
    at: SurfaceElevation,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The surface.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, OverlayStyle::CSS);
    let depth = Some(at.depth().to_string());
    view! {
        box(
            class = "zui-overlay-depth",
            var:--zui-overlay-depth = {depth},
            {..attrs},
            class = class
        ) {
            {children.view()}
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui::prelude::*;
    use zgui::reactive::{Mounted, install};

    use super::SurfaceElevation;

    #[test]
    fn a_surface_outside_every_other_one_keeps_the_band_it_asked_for() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            assert_eq!(SurfaceElevation::current(), None);
            let raised = SurfaceElevation::raise(OverlayLayer::Popover);
            assert_eq!(raised.layer(), OverlayLayer::Popover);
            assert_eq!(raised.depth(), 0);
        });
        scope.unmount();
    }

    #[test]
    fn a_surface_inside_a_higher_one_rises_to_it_and_a_higher_one_stays_where_it_is() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            SurfaceElevation::raise(OverlayLayer::Modal).publish();
            let list = SurfaceElevation::raise(OverlayLayer::Popover);
            assert_eq!(
                list.layer(),
                OverlayLayer::Modal,
                "a select opened inside a dialog belongs on the dialog's band"
            );
            assert_eq!(list.depth(), 1, "and above the dialog on it");
            assert_eq!(
                SurfaceElevation::raise(OverlayLayer::Toast).layer(),
                OverlayLayer::Toast,
                "a notice raised from inside a dialog is still above it"
            );
        });
        scope.unmount();
    }

    #[test]
    fn the_floor_is_what_the_last_surface_ended_up_on_rather_than_what_it_asked_for() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            SurfaceElevation::raise(OverlayLayer::Modal).publish();
            SurfaceElevation::raise(OverlayLayer::Popover).publish();
            let menu = SurfaceElevation::raise(OverlayLayer::Popover);
            assert_eq!(
                menu.layer(),
                OverlayLayer::Modal,
                "a menu opened from a select that is itself inside a dialog stays above the dialog"
            );
            assert_eq!(menu.depth(), 2, "and one step above the select");
        });
        scope.unmount();
    }
}
