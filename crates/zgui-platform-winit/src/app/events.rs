//! Turning one window event into the one thing that happened.

use winit::event::WindowEvent;
use zgui_geom::{DevicePx, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_vocab::{PointerAction, Timestamp};

use crate::app::window::WindowState;
use crate::input::{ime, keyboard, pointer, wheel};
use crate::surface::WinitSurface;
use crate::theme;

/// What happened to `surface`, in the contract's vocabulary.
///
/// Most window events are one thing that happened and cross straight over. Four of them are not,
/// and each is handled here rather than above:
///
/// * a **held-modifier change** is a state the contract carries and the platform reports only when
///   it moves, so the new set is remembered as it passes;
/// * a **pointer position** is remembered for the same reason, because a wheel turn and a file drop
///   arrive without one and both have to be routed to whatever is under the pointer;
/// * a **dragged file** is one of a set the platform reports one at a time, so it is gathered
///   rather than announced (see [`Drag`](crate::app::drag::Drag));
/// * a **synthetic key event** — the platform's way of reporting which keys were already held when
///   a window gained focus — is dropped, because dispatching it would type a character nobody
///   pressed.
pub(crate) fn translate(
    surface: &WinitSurface,
    state: &mut WindowState,
    timestamp: Timestamp,
    event: WindowEvent,
) -> Option<SurfaceEvent> {
    let scale = surface.scale();
    match event {
        WindowEvent::Resized(size) => Some(SurfaceEvent::Resized(Size::new(
            DevicePx(size.width as f32),
            DevicePx(size.height as f32),
        ))),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            // The size that comes with it is the window's own, read after the change: a scale
            // change and the resize it causes are one event to anything that has to redraw.
            Some(SurfaceEvent::ScaleFactorChanged {
                scale_factor,
                size: Surface::size(surface),
            })
        }
        WindowEvent::CloseRequested => Some(SurfaceEvent::CloseRequested),
        WindowEvent::Destroyed => Some(SurfaceEvent::Destroyed),
        WindowEvent::Focused(focused) => Some(SurfaceEvent::Focused(focused)),
        WindowEvent::Occluded(occluded) => Some(SurfaceEvent::Occluded(occluded)),
        WindowEvent::ThemeChanged(changed) => {
            Some(SurfaceEvent::ColorSchemeChanged(theme::scheme(changed)))
        }
        WindowEvent::RedrawRequested => Some(SurfaceEvent::RedrawRequested),
        WindowEvent::ModifiersChanged(changed) => {
            state.modifiers = keyboard::modifiers(changed.state());
            Some(SurfaceEvent::ModifiersChanged(state.modifiers))
        }
        WindowEvent::KeyboardInput {
            event,
            is_synthetic,
            ..
        } => {
            if is_synthetic {
                return None;
            }
            Some(SurfaceEvent::Key {
                state: keyboard::state(event.state),
                event: keyboard::event(&event),
                modifiers: state.modifiers,
                timestamp,
            })
        }
        WindowEvent::CursorMoved { position, .. } => {
            state.pointer = pointer::position(position, scale);
            Some(SurfaceEvent::Pointer {
                action: PointerAction::Moved,
                event: pointer::mouse(state.pointer, None),
                modifiers: state.modifiers,
                timestamp,
            })
        }
        WindowEvent::CursorEntered { .. } => Some(SurfaceEvent::Pointer {
            action: PointerAction::Entered,
            event: pointer::mouse(state.pointer, None),
            modifiers: state.modifiers,
            timestamp,
        }),
        WindowEvent::CursorLeft { .. } => Some(SurfaceEvent::Pointer {
            action: PointerAction::Left,
            event: pointer::mouse(state.pointer, None),
            modifiers: state.modifiers,
            timestamp,
        }),
        WindowEvent::MouseInput {
            state: pressed,
            button,
            ..
        } => Some(SurfaceEvent::Pointer {
            action: match pressed {
                winit::event::ElementState::Pressed => PointerAction::Pressed,
                winit::event::ElementState::Released => PointerAction::Released,
            },
            event: pointer::mouse(state.pointer, Some(pointer::button(button))),
            modifiers: state.modifiers,
            timestamp,
        }),
        WindowEvent::MouseWheel { delta, phase, .. } => Some(SurfaceEvent::Wheel {
            event: wheel::event(
                wheel::delta(delta, scale),
                wheel::phase(delta, phase),
                state.pointer,
            ),
            modifiers: state.modifiers,
            timestamp,
        }),
        WindowEvent::Touch(contact) => {
            let event = pointer::touch(&contact, scale);
            state.pointer = event.position;
            Some(SurfaceEvent::Pointer {
                action: pointer::action(contact.phase),
                event,
                modifiers: state.modifiers,
                timestamp,
            })
        }
        WindowEvent::Ime(composition) => Some(SurfaceEvent::Ime(ime::event(composition))),
        WindowEvent::HoveredFile(path) => {
            state.drag.hovering(path);
            None
        }
        WindowEvent::DroppedFile(path) => {
            state.drag.dropped(path);
            None
        }
        WindowEvent::HoveredFileCancelled => {
            state.drag.left();
            None
        }
        // Everything else is either about the window's place on the desktop, which nothing above
        // this layer asks about, or a gesture the platform recognises on its own and this framework
        // synthesises from the pointer stream instead.
        _ => None,
    }
}
