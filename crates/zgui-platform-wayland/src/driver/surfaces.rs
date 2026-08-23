//! Making a surface, and taking one down.

use std::sync::Arc;

use zgui_geom::{Css, CssPx, Size};
use zgui_platform::{
    LayerPlacement, PlatformError, PopupPlacement, SurfaceAttributes, SurfaceId, SurfaceRole,
    Unsupported,
};

use crate::driver::WaylandState;
use crate::surface::role::{Popup, Role, Toplevel, layer};
use crate::surface::{Fractional, Links, WaylandSurface};

/// The extent a surface opens at when the application named none.
const DEFAULT_SIZE: Size<CssPx, Css> = Size::new(CssPx(800.0), CssPx(600.0));

impl WaylandState {
    /// Creates a surface in whatever role the attributes ask for.
    ///
    /// The order is the protocol's and none of it is optional: make the surface, give it a role,
    /// attach the scaling objects, then commit **with no buffer**. That last commit is what asks
    /// the compositor to configure the surface, and the first frame cannot be drawn until it
    /// answers. Attaching a buffer before then is a protocol error.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Unsupported`] for a role this compositor has no protocol for, and
    /// [`PlatformError::SurfaceCreation`] when a pop-up's parent has already gone.
    pub(crate) fn make_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<WaylandSurface>, PlatformError> {
        let id = self.live.next_id();
        if let Some(named) = &attributes.application_id {
            self.live
                .app_id
                .borrow_mut()
                .get_or_insert_with(|| named.as_str().to_owned());
        }
        let wanted = attributes.size.unwrap_or(DEFAULT_SIZE);
        let wl = self.compositor.create_surface(&self.qh);
        let fractional = Fractional::attach(&self.extras, &self.qh, &wl, id);
        let role = self.make_role(id, attributes, wl)?;

        let (placement, parent_extent) = match &attributes.role {
            SurfaceRole::Popup(placement) => (
                Some(placement.clone()),
                self.configured_extent(placement.parent).unwrap_or(wanted),
            ),
            _ => (None, wanted),
        };
        let surface = WaylandSurface::new(
            id,
            role,
            fractional,
            wanted,
            placement,
            parent_extent,
            self.links(),
        );
        apply(&surface, attributes);
        // The commit that asks to be configured. Nothing is attached to it, which is what makes it
        // the initial commit rather than a frame.
        surface.role().commit();
        self.live.surfaces.borrow_mut().push(Arc::clone(&surface));
        Ok(surface)
    }

    /// The shell object for the role the attributes ask for.
    fn make_role(
        &self,
        id: SurfaceId,
        attributes: &SurfaceAttributes,
        wl: wayland_client::protocol::wl_surface::WlSurface,
    ) -> Result<Role, PlatformError> {
        match &attributes.role {
            SurfaceRole::Layer(placement) => self.make_layer(wl, placement),
            SurfaceRole::Popup(placement) => self.make_popup(id, wl, placement, attributes),
            _ => Ok(Role::Toplevel(Box::new(Toplevel::new(
                &self.xdg,
                &self.qh,
                wl,
                id,
                attributes.decorations,
            )))),
        }
    }

    /// A surface placed against a rectangle of another, dismissed with it.
    ///
    /// The positioner is filled in against the parent's *logical* extent, because that is the
    /// space the anchor rectangle arrives in and the space the compositor measures it in. A
    /// rectangle outside it, or one of no extent, is a protocol error rather than a placement the
    /// compositor corrects, so both are clamped before they are sent.
    fn make_popup(
        &self,
        id: SurfaceId,
        wl: wayland_client::protocol::wl_surface::WlSurface,
        placement: &PopupPlacement,
        attributes: &SurfaceAttributes,
    ) -> Result<Role, PlatformError> {
        let Some(parent) = self.live.surface(placement.parent) else {
            return Err(PlatformError::SurfaceCreation(
                "the surface this pop-up belongs to has already gone".to_owned(),
            ));
        };
        let Some(anchor) = parent.role().xdg_surface() else {
            return Err(PlatformError::SurfaceCreation(
                "a pop-up can only be placed against a window or another pop-up".to_owned(),
            ));
        };
        let extent = parent.shared().logical;
        let size = attributes.size.unwrap_or(extent);
        Ok(Role::Popup(Box::new(Popup::new(
            &self.xdg,
            &self.qh,
            wl,
            id,
            anchor,
            &crate::surface::role::xdg::popup::Placed {
                place: placement,
                parent: extent,
                size,
            },
        ))))
    }

    /// A surface that is part of the desktop shell rather than a window.
    fn make_layer(
        &self,
        wl: wayland_client::protocol::wl_surface::WlSurface,
        placement: &LayerPlacement,
    ) -> Result<Role, PlatformError> {
        let Some(shell) = &self.layers else {
            return Err(PlatformError::Unsupported(Unsupported));
        };
        let output = placement.monitor.as_ref().and_then(|name| {
            self.outputs.outputs().find(|output| {
                self.outputs
                    .info(output)
                    .and_then(|info| info.name)
                    .is_some_and(|found| &found == name)
            })
        });
        let surface = shell.create_layer_surface(
            &self.qh,
            wl,
            layer::layer(placement.layer),
            Some("zgui"),
            output.as_ref(),
        );
        layer::apply(&surface, placement);
        Ok(Role::Layer(Box::new(surface)))
    }

    /// What every surface this loop makes is wired into.
    fn links(&self) -> Links {
        Links {
            conn: self.conn.clone(),
            qh: self.qh.clone(),
            compositor: self.compositor.wl_compositor().clone(),
            presentation: self.presentation.clone(),
            ping: self.live.waker.ping(),
            seat: std::sync::Arc::clone(&self.link),
            shell: std::sync::Arc::clone(&self.xdg),
            waker: std::sync::Arc::clone(&self.live.waker)
                as std::sync::Arc<dyn zgui_platform::Waker>,
        }
    }

    /// Takes a surface down, innermost object first.
    pub(crate) fn destroy_surface(&self, id: SurfaceId) {
        self.live.close(id);
    }
}

/// Applies the attributes that are requests rather than creation arguments.
fn apply(surface: &Arc<WaylandSurface>, attributes: &SurfaceAttributes) {
    if let Some(window) = surface.role().window() {
        window.set_title(attributes.title.as_str());
        if let Some(id) = &attributes.application_id {
            window.set_app_id(id.as_str());
        }
        if attributes.maximized {
            window.set_maximized(true);
        }
        if attributes.fullscreen.is_some() {
            window.set_fullscreen(true);
        }
        // Both bounds in one request, because a window that is not resizable is one whose smallest
        // and largest extents are the same and the two have to agree.
        if attributes.resizable {
            window.set_bounds(attributes.min_size, attributes.max_size);
        } else {
            let fixed = attributes.size;
            window.set_bounds(fixed, fixed);
        }
    }
    if let Some(size) = attributes.size
        && let Some(layer) = surface.role().layer()
    {
        layer.set_size(
            size.width.0.round().max(1.0) as u32,
            size.height.0.round().max(1.0) as u32,
        );
    }
}
