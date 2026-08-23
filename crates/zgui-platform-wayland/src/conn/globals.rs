//! The globals this backend binds itself, beside the ones the toolkit owns.

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Dispatch, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use crate::driver::WaylandState;

/// The optional protocols the toolkit has no wrapper for.
///
/// Both are how a surface is drawn at a scale that is not a whole number, and they only work
/// together: the fractional scale says what the compositor wants, and the viewport is the only way
/// to say that a buffer of one extent should appear at another. A compositor offering one without
/// the other leaves the surface on whole-number scaling, which is why they are read as a pair.
#[derive(Debug, Default)]
pub struct Extras {
    /// `wp_fractional_scale_manager_v1`: what scale the compositor wants for a surface.
    pub fractional_scale: Option<WpFractionalScaleManagerV1>,
    /// `wp_viewporter`: the mapping from a buffer's extent to the extent it appears at.
    pub viewporter: Option<WpViewporter>,
}

impl Extras {
    /// Binds whichever of these the compositor advertised.
    ///
    /// A missing global is not an error. Every one here is an improvement on a path that already
    /// works, so a compositor without them gets whole-number scaling rather than no window.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<WaylandState>) -> Self {
        let fractional_scale = bind(globals, qh);
        let viewporter = bind(globals, qh);
        if fractional_scale.is_some() != viewporter.is_some() {
            tracing::debug!(
                fractional_scale = fractional_scale.is_some(),
                viewporter = viewporter.is_some(),
                "fractional scaling needs both globals; falling back to whole-number scaling"
            );
        }
        Self {
            fractional_scale,
            viewporter,
        }
    }

    /// Whether this compositor can scale a surface by something other than a whole number.
    pub const fn scales_fractionally(&self) -> bool {
        self.fractional_scale.is_some() && self.viewporter.is_some()
    }
}

/// Binds one optional global at version one.
fn bind<I>(globals: &GlobalList, qh: &QueueHandle<WaylandState>) -> Option<I>
where
    I: wayland_client::Proxy + 'static,
    WaylandState: Dispatch<I, GlobalData>,
{
    globals.bind(qh, 1..=1, GlobalData).ok()
}
