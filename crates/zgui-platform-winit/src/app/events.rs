//! Turning one window event into the one thing that happened.

use winit::event::WindowEvent;
use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_vocab::{PointerAction, Timestamp};

use crate::app::window::WindowState;
use crate::input::{ime, keyboard, pointer, wheel};
use crate::theme;

/// What happened to one window, in the contract's vocabulary.
///
/// `state` is what the window remembers between events. Keep one [`WindowState`] for each window
/// and give every event of that window to this function in the order it arrived.
/// `scale_factor` is the window's own scale, which converts physical pixels to CSS pixels. `size`
/// returns the window's extent in device pixels, and is called only for a scale change.
///
/// This function needs no [`WinitSurface`](crate::surface::WinitSurface), so a program that owns
/// its own winit loop can translate that loop's events with it. Dragged files are gathered in
/// [`WindowState::drag`]; take them with [`Drag::take`](crate::app::drag::Drag::take) at the end of
/// the loop's turn.
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
pub fn translate(
    state: &mut WindowState,
    scale_factor: f64,
    size: impl FnOnce() -> Size<DevicePx, Device>,
    timestamp: Timestamp,
    event: WindowEvent,
) -> Option<SurfaceEvent> {
    let scale = scale_factor;
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
                size: size(),
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
        } => {
            let pressed = pressed == winit::event::ElementState::Pressed;
            let (button, modifiers) = pointer::context_click(
                &mut state.context_click,
                pressed,
                pointer::button(button),
                state.modifiers,
            );
            Some(SurfaceEvent::Pointer {
                action: if pressed {
                    PointerAction::Pressed
                } else {
                    PointerAction::Released
                },
                event: pointer::mouse(state.pointer, Some(button)),
                modifiers,
                timestamp,
            })
        }
        WindowEvent::MouseWheel { delta, phase, .. } => Some(SurfaceEvent::Wheel {
            event: wheel::event(
                wheel::delta(delta, scale),
                scroll_phase(delta, phase, scale),
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

/// Where in a gesture a scroll sits, with the momentum phase AppKit reports.
#[cfg(target_os = "macos")]
fn scroll_phase(
    delta: winit::event::MouseScrollDelta,
    phase: winit::event::TouchPhase,
    scale: f64,
) -> zgui_vocab::ScrollPhase {
    crate::macos::scroll_phase(delta, phase, scale)
}

/// Where in a gesture a scroll sits.
#[cfg(not(target_os = "macos"))]
fn scroll_phase(
    delta: winit::event::MouseScrollDelta,
    phase: winit::event::TouchPhase,
    _scale: f64,
) -> zgui_vocab::ScrollPhase {
    wheel::phase(delta, phase)
}

#[cfg(test)]
mod tests {
    use super::translate;
    use crate::app::window::WindowState;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};
    use winit::keyboard::ModifiersState;
    use zgui_geom::{CssPx, DevicePx, Point, Size};
    use zgui_platform::SurfaceEvent;
    use zgui_vocab::{Modifiers, PointerAction, ScrollDelta, Timestamp};

    /// Translates `event` for a window at scale two that is never asked for its size.
    fn at_scale_two(state: &mut WindowState, event: WindowEvent) -> Option<SurfaceEvent> {
        translate(
            state,
            2.0,
            || panic!("only a scale change reads the size"),
            Timestamp::ORIGIN,
            event,
        )
    }

    #[test]
    fn a_pointer_is_placed_in_css_pixels_and_remembered_for_the_wheel() {
        let mut state = WindowState::default();
        let moved = at_scale_two(
            &mut state,
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(40.0, 20.0),
            },
        );
        let expected = Point::new(CssPx(20.0), CssPx(10.0));
        match moved {
            Some(SurfaceEvent::Pointer { action, event, .. }) => {
                assert_eq!(action, PointerAction::Moved);
                assert_eq!(event.position, expected);
            }
            other => panic!("a cursor move crossed as {other:?}"),
        }
        assert_eq!(state.pointer, expected);

        let wheel = at_scale_two(
            &mut state,
            WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, 1.0),
                phase: TouchPhase::Moved,
            },
        );
        match wheel {
            Some(SurfaceEvent::Wheel { event, .. }) => {
                assert_eq!(
                    event.position, expected,
                    "a wheel turn goes where the pointer is"
                );
                assert_eq!(event.delta, ScrollDelta::Lines { x: -0.0, y: -1.0 });
            }
            other => panic!("a wheel turn crossed as {other:?}"),
        }
    }

    #[test]
    fn held_modifiers_are_remembered_and_carried_by_later_events() {
        let mut state = WindowState::default();
        let changed = at_scale_two(
            &mut state,
            WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        );
        assert!(
            matches!(changed, Some(SurfaceEvent::ModifiersChanged(held)) if held == Modifiers::CONTROL),
            "{changed:?}"
        );
        let entered = at_scale_two(
            &mut state,
            WindowEvent::CursorEntered {
                device_id: DeviceId::dummy(),
            },
        );
        match entered {
            Some(SurfaceEvent::Pointer { modifiers, .. }) => {
                assert_eq!(modifiers, Modifiers::CONTROL);
            }
            other => panic!("a cursor entry crossed as {other:?}"),
        }
    }

    #[test]
    fn a_resize_crosses_in_device_pixels_and_dropped_files_are_gathered() {
        let mut state = WindowState::default();
        let resized = at_scale_two(
            &mut state,
            WindowEvent::Resized(PhysicalSize::new(640, 480)),
        );
        assert!(
            matches!(resized, Some(SurfaceEvent::Resized(size)) if size == Size::new(DevicePx(640.0), DevicePx(480.0))),
            "{resized:?}"
        );

        let dropped = at_scale_two(&mut state, WindowEvent::DroppedFile("/a.png".into()));
        assert!(
            dropped.is_none(),
            "a dropped file waits for the rest of its set"
        );
        assert!(state.drag.is_pending());
        let events = state.drag.take(state.pointer);
        assert_eq!(events.len(), 1);
        assert!(events[0].is_drop());
    }
}
