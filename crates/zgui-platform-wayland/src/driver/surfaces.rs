//! Making a surface, and taking one down.

use std::sync::Arc;

use zgui_geom::{Css, CssPx, Size};
use zgui_platform::{
    LayerPlacement, PlatformError, PopupPlacement, SurfaceAttributes, SurfaceId, SurfaceRole,
    Unsupported,
};

use wayland_client::protocol::wl_output::WlOutput;

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
        let bound = match &attributes.role {
            SurfaceRole::Layer(placement) => placement
                .monitor
                .as_ref()
                .map(|target| self.output_for(target))
                .transpose()?,
            SurfaceRole::Lock(target) => Some(self.output_for(target)?),
            _ => None,
        };
        let wl = self.compositor.create_surface(&self.qh);
        let fractional = Fractional::attach(&self.extras, &self.qh, &wl, id);
        let seed = self.seed_scale(bound.as_ref(), fractional.crops());
        let role = self.make_role(id, attributes, wanted, wl, bound.as_ref())?;

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
        {
            let mut shared = surface.shared();
            shared.ahead =
                crate::frame::Ahead::new(crate::frame::ahead::mode(attributes.compose_ahead));
            if let SurfaceRole::Layer(placement) = &attributes.role {
                shared.layer = Some(placement.clone());
            }
            shared.bound = bound;
            // The first frame is drawn at the scale of the output the surface opens on. The
            // compositor states the surface's own scale only after it has shown the surface, so
            // without this a surface on a 2x output draws its first frame at 1x and then again.
            shared.ladder.seed(seed);
            shared.scale = shared.ladder.factor();
        }
        apply(&surface, attributes);
        // The commit that asks to be configured. Nothing is attached to it, which makes it the
        // initial commit. A lock surface is configured unasked, and a commit before it
        // acknowledges that configure is a protocol error.
        if surface.role().lock().is_none() {
            surface.role().commit();
        }
        self.live.surfaces.borrow_mut().push(Arc::clone(&surface));
        Ok(surface)
    }

    /// The shell object for the role the attributes ask for.
    fn make_role(
        &self,
        id: SurfaceId,
        attributes: &SurfaceAttributes,
        wanted: Size<CssPx, Css>,
        wl: wayland_client::protocol::wl_surface::WlSurface,
        bound: Option<&WlOutput>,
    ) -> Result<Role, PlatformError> {
        match &attributes.role {
            SurfaceRole::Layer(placement) => {
                let application = attributes
                    .application_id
                    .as_ref()
                    .map(zgui_vocab::SharedString::as_str);
                self.make_layer(wl, placement, application, wanted, bound)
            }
            SurfaceRole::Popup(placement) => self.make_popup(id, wl, placement, attributes),
            SurfaceRole::Lock(_) => {
                let Some(output) = bound else {
                    return Err(PlatformError::SurfaceCreation(
                        "a lock surface needs an output".to_owned(),
                    ));
                };
                self.locks
                    .surface(wl, output, &self.qh)
                    .map(|lock| Role::Lock(Box::new(lock)))
            }
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
    ///
    /// A window or another pop-up is the pop-up's parent directly. A shell layer adopts a pop-up
    /// made with no parent. The adoption and the grab both precede the first commit, which the
    /// protocol requires of each.
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
        let layer = parent.role().layer();
        let anchor = parent.role().xdg_surface();
        if layer.is_none() && anchor.is_none() {
            return Err(PlatformError::SurfaceCreation(
                "a pop-up can only be placed against a window, a pop-up or a shell layer"
                    .to_owned(),
            ));
        }
        let extent = parent.shared().logical;
        let size = attributes.size.unwrap_or(extent);
        let popup = Popup::new(
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
        );
        if let Some(layer) = layer {
            layer.get_popup(popup.xdg_popup());
        }
        if let Some(serial) = placement.grab {
            match &self.seat.serials.seat {
                Some(seat) => popup.grab(seat, serial.raw()),
                None => tracing::debug!(?serial, "a pop-up grab with no seat to grab on"),
            }
        }
        Ok(Role::Popup(Box::new(popup)))
    }

    /// A surface that is part of the desktop shell rather than a window.
    ///
    /// The namespace is the placement's own, or else the application identifier. A compositor's
    /// layer rules match it, so two kinds of shell surface from one application stay apart only
    /// when each names its own.
    fn make_layer(
        &self,
        wl: wayland_client::protocol::wl_surface::WlSurface,
        placement: &LayerPlacement,
        application: Option<&str>,
        wanted: Size<CssPx, Css>,
        bound: Option<&WlOutput>,
    ) -> Result<Role, PlatformError> {
        let Some(shell) = &self.layers else {
            return Err(PlatformError::Unsupported(Unsupported));
        };
        let surface = shell.create_layer_surface(
            &self.qh,
            wl,
            layer::layer(placement.layer),
            Some(layer::namespace(placement, application)),
            bound,
        );
        layer::apply(&surface, placement, wanted);
        Ok(Role::Layer(Box::new(surface)))
    }

    /// The output a shell layer or a lock surface asks to open on.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::SurfaceCreation`] when `target` names an output that is not
    /// connected. The desktop would otherwise choose an output, and a bar opened on the wrong
    /// screen is a second bar on that screen.
    fn output_for(&self, target: &zgui_platform::MonitorTarget) -> Result<WlOutput, PlatformError> {
        self.outputs
            .outputs()
            .find(|output| {
                self.outputs
                    .info(output)
                    .is_some_and(|info| target.matches(&crate::output::describe(&info)))
            })
            .ok_or_else(|| {
                PlatformError::SurfaceCreation(format!("no connected output matches {target:?}"))
            })
    }

    /// The scale a new surface starts at: its bound output's, else the sharpest output's.
    ///
    /// A surface with no bound output can open on any output, and the sharpest one is the one
    /// where drawing at too low a scale shows.
    fn seed_scale(&self, bound: Option<&WlOutput>, viewport: bool) -> Option<f64> {
        match bound {
            Some(output) => self
                .outputs
                .info(output)
                .map(|info| crate::output::mode::seed(&info, viewport)),
            None => crate::output::mode::sharpest(
                self.outputs
                    .outputs()
                    .filter_map(|output| self.outputs.info(&output))
                    .collect::<Vec<_>>()
                    .iter(),
            )
            .map(f64::from),
        }
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
}
