//! Asking the desktop to bring a window forward, or to draw attention to it.
//!
//! Both are the same protocol and the same two steps: ask for a token, then hand the token back
//! with the surface it is for. What the compositor does with it is its own business — bring the
//! window forward, flash a task-bar entry, or nothing at all — and a client cannot tell which.
//! That is not a limitation to be worked around: focus-stealing prevention is the whole reason the
//! protocol has this shape, and an application that could take focus whenever it liked would take
//! it from whatever the person is typing into.

use smithay_client_toolkit::activation::{ActivationHandler, RequestData};
use smithay_client_toolkit::delegate_activation;
use zgui_platform::SurfaceId;

use crate::driver::WaylandState;

/// What a surface asked the desktop for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wanted {
    /// The window should be brought forward.
    Focus,
    /// The desktop should draw attention to the window.
    Attention,
}

impl WaylandState {
    /// Carries out every activation a surface asked for since the last turn.
    pub(crate) fn carry_out_activations(&mut self) {
        for (surface, focus) in self.link.take_activations() {
            self.activate(
                surface,
                if focus {
                    Wanted::Focus
                } else {
                    Wanted::Attention
                },
            );
        }
    }

    /// Asks for a token so that `surface` can be activated.
    ///
    /// The seat and the serial of the last press are quoted, because many compositors issue a
    /// token that does nothing for a request without one — and none of them says so. Asking anyway
    /// is still right: a token that turns out to be refused costs one round trip, and the
    /// alternative is a window that never comes forward even when the person asked it to.
    pub(crate) fn activate(&mut self, surface: SurfaceId, wanted: Wanted) {
        let Some(activation) = &self.activation else {
            return;
        };
        let Some(held) = self.live.surface(surface) else {
            return;
        };
        self.wanted.insert(surface, wanted);
        activation.request_token(
            &self.qh,
            RequestData {
                app_id: self.live.app_id.borrow().clone(),
                seat_and_serial: self.seat.serials.seat.clone().zip(self.seat.serials.grab()),
                surface: Some(held.wl_surface().clone()),
            },
        );
    }
}

impl ActivationHandler for WaylandState {
    type RequestData = RequestData;

    fn new_token(&mut self, token: String, data: &Self::RequestData) {
        let Some(activation) = &self.activation else {
            return;
        };
        let Some(surface) = data
            .surface
            .as_ref()
            .and_then(|surface| self.identify(surface))
        else {
            return;
        };
        // Whether the window comes forward or merely lights up is the compositor's decision, and
        // the request is the same either way. What is different is what the application asked for,
        // and it is dropped here rather than remembered: a token is good once.
        self.wanted.remove(&surface);
        if let Some(held) = self.live.surface(surface) {
            activation.activate::<Self>(held.wl_surface(), token);
        }
    }
}

delegate_activation!(WaylandState);
