//! Dragging a window by something it drew itself.
//!
//! A window that turns the desktop's title bar off has to provide what the title bar provided: a
//! place to drag it by, edges to resize from, and a way to maximise it. None of that can be done by
//! moving the window from inside the application — a Wayland compositor does not let a window place
//! itself at all — so each one asks the desktop to take over the drag instead, which is what these
//! handlers do.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_view::event::{EventCx, events, handler};
use zgui_vocab::{PointerButton, Timestamp};

use crate::windows::WindowHandle;

/// How long after a press a second press is the same gesture.
///
/// The desktop convention rather than a preference read from one, because no platform exposes it
/// through winit. Long enough for a deliberate double press, short enough that two separate drags
/// are never mistaken for one.
const DOUBLE_PRESS: Duration = Duration::from_millis(400);

/// How far a second press may be from the first and still be the same gesture, in CSS pixels.
const SLOP: f32 = 4.0;

impl WindowHandle {
    /// A `pointer_down` handler that drags the window, for a title bar the application drew.
    ///
    /// A primary press starts a desktop-driven move. A second primary press inside the double-press
    /// interval maximises the window instead, or restores it — the convention every desktop's own
    /// title bar follows.
    ///
    /// The double press is detected press-to-press rather than through the ordinary click path,
    /// because once the move begins the compositor owns the pointer: no release arrives, so no
    /// click and no double click can ever be formed from it.
    ///
    /// A control *inside* the title bar needs [`WindowHandle::no_drag_handler`], or its press
    /// reaches this one and starts a drag instead of pressing the control. Nothing here fires for a
    /// secondary press, so right-clicking a title bar still reaches whatever context menu the
    /// application put there.
    ///
    /// ```no_run
    /// # use zgui_runtime::windows::use_window;
    /// # fn example() {
    /// let window = use_window();
    /// // row(class = "titlebar", on:pointer_down = window.move_drag_handler()) { … }
    /// # let _ = window.move_drag_handler();
    /// # }
    /// ```
    pub fn move_drag_handler(&self) -> impl Fn(&mut EventCx<'_, events::PointerDown>) + 'static {
        let window = self.clone();
        let last: Rc<Cell<Option<(Timestamp, Point<CssPx, Css>)>>> = Rc::new(Cell::new(None));
        handler(
            events::POINTER_DOWN,
            move |ev: &mut EventCx<'_, events::PointerDown>| {
                if ev.button != Some(PointerButton::Primary) || !ev.primary {
                    return;
                }
                ev.stop_propagation();
                let now = ev.timestamp;
                let at = ev.position;
                let again = last.get().is_some_and(|(when, previous)| {
                    now.saturating_since(when) <= DOUBLE_PRESS
                        && (previous.x.0 - at.x.0).abs() <= SLOP
                        && (previous.y.0 - at.y.0).abs() <= SLOP
                });
                if again {
                    // The gesture is over either way: a third press is a new one.
                    last.set(None);
                    window.toggle_maximized();
                } else {
                    last.set(Some((now, at)));
                    window.begin_move_drag();
                }
            },
        )
    }

    /// A `pointer_down` handler that keeps a press from reaching the title bar around it.
    ///
    /// Every control inside a self-drawn title bar needs one. A press bubbles from the control it
    /// landed on outwards to the bar, and the bar's own handler starts a desktop-driven move — at
    /// which point the compositor owns the pointer, no release ever arrives, and the click the
    /// control was waiting for is never formed. The symptom is a title bar whose buttons do nothing
    /// at all.
    ///
    /// The click still happens: this stops the press from travelling further, not the control from
    /// answering it.
    ///
    /// ```no_run
    /// # use zgui_runtime::windows::use_window;
    /// # fn example() {
    /// let window = use_window();
    /// // control(on:pointer_down = window.no_drag_handler(), on:click = …) { "✕" }
    /// # let _ = window.no_drag_handler();
    /// # }
    /// ```
    pub fn no_drag_handler(
        &self,
    ) -> impl Fn(&mut EventCx<'_, events::PointerDown>) + Copy + 'static {
        handler(
            events::POINTER_DOWN,
            move |ev: &mut EventCx<'_, events::PointerDown>| ev.stop_propagation(),
        )
    }

    /// A `pointer_down` handler that resizes the window from one edge or corner.
    ///
    /// What an undecorated window puts on its own borders. Does nothing on a desktop that will not
    /// let an application start a resize, which is macOS.
    ///
    /// The cursor is not set from here: which cursor an edge wants is a property of the element,
    /// and setting it on the press would change it after the drag had already begun. A border sets
    /// its own on `pointer_enter` through [`WindowHandle::set_cursor`].
    pub fn resize_drag_handler(
        &self,
        edge: zgui_platform::ResizeEdge,
    ) -> impl Fn(&mut EventCx<'_, events::PointerDown>) + 'static {
        let window = self.clone();
        handler(
            events::POINTER_DOWN,
            move |ev: &mut EventCx<'_, events::PointerDown>| {
                if ev.button != Some(PointerButton::Primary) || !ev.primary {
                    return;
                }
                ev.stop_propagation();
                window.begin_resize_drag(edge);
            },
        )
    }
}
