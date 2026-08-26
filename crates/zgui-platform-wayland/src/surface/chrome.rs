//! The requests a surface makes about itself.
//!
//! Split out of the trait so that each one reads as what it does to the compositor rather than as
//! one arm of a very long implementation. Every function here is the whole of one contract method.

use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::Unsupported;

use crate::surface::WaylandSurface;

/// Asks for a different content extent, and reports what it will be drawn at.
///
/// A Wayland client is not granted a size; it draws at one and the compositor accepts it. So this
/// takes effect rather than being requested, and the frame that follows is the one that shows it.
/// A compositor that disagrees says so with the next configure, which wins.
pub fn request_size(
    surface: &WaylandSurface,
    size: Size<CssPx, Css>,
) -> Option<Size<DevicePx, Device>> {
    if size.width.0 <= 0.0 || size.height.0 <= 0.0 {
        return None;
    }
    let scale = {
        let mut shared = surface.shared();
        let scale = shared.scale;
        shared.resized(size, scale);
        scale
    };
    if let Some(window) = surface.role().window() {
        window.set_geometry(size);
    }
    if let Some(layer) = surface.role().layer() {
        layer.set_size(
            size.width.0.round().max(1.0) as u32,
            size.height.0.round().max(1.0) as u32,
        );
    }
    // A pop-up is placed rather than sized: growing one means asking the compositor to work out
    // where a rectangle of the new extent fits, which is the same question it answered when the
    // pop-up opened. Moving it is what keeps a menu that grew an item from flickering shut and
    // open again; where the shell is too old to move one, the caller closes it and opens another.
    if surface.role().popup().is_some() {
        surface.reposition(size);
    }
    zgui_platform::Surface::request_redraw(surface);
    Some(Size::new(
        DevicePx((size.width.0 as f64 * scale).round() as f32),
        DevicePx((size.height.0 as f64 * scale).round() as f32),
    ))
}

/// Sets the smallest extent the user may drag to.
///
/// Both bounds go out together, because the shell takes them as two requests and a window that
/// stated only one has left the other at whatever it was — including at the pair a
/// [`set_resizable`] call had pinned it to.
pub fn set_min_size(surface: &WaylandSurface, size: Option<Size<CssPx, Css>>) {
    let mut bounds = surface.bounds();
    bounds.0 = size;
    surface.set_bounds(bounds);
}

/// Sets the largest extent the user may drag to.
pub fn set_max_size(surface: &WaylandSurface, size: Option<Size<CssPx, Css>>) {
    let mut bounds = surface.bounds();
    bounds.1 = size;
    surface.set_bounds(bounds);
}

/// Allows or forbids the user resizing the surface.
///
/// There is no such request in the protocol. A window is made unresizable by telling the
/// compositor its smallest and largest extents are the one it is at, which is what every toolkit
/// on this desktop does and what a compositor's resize handles read.
pub fn set_resizable(surface: &WaylandSurface, resizable: bool) {
    if resizable {
        let bounds = surface.bounds();
        surface.set_bounds(bounds);
        return;
    }
    let fixed = Some(surface.shared().logical);
    if let Some(window) = surface.role().window() {
        window.set_bounds(fixed, fixed);
    }
}

/// Shows or hides the surface.
///
/// A surface here is mapped by the first commit that carries a buffer, so showing one is what the
/// first frame already does and there is nothing further to ask for. Hiding is a commit with no
/// buffer, which unmaps it — and the compositor then treats the next buffer as a fresh mapping,
/// configure sequence and all.
pub fn set_visible(surface: &WaylandSurface, visible: bool) {
    if visible {
        zgui_platform::Surface::request_redraw(surface);
        return;
    }
    let wl = surface.wl_surface();
    wl.attach(None, 0, 0);
    wl.commit();
    let mut shared = surface.shared();
    shared.visibility.configured = false;
}

/// Makes the surface transparent to the pointer, or takes it back.
///
/// An empty input region is the protocol's way of saying "nothing here is mine". It is committed
/// at once rather than with the next frame: a region that waits for a frame would let through the
/// very click it was set for.
pub fn set_pointer_passthrough(
    surface: &WaylandSurface,
    passthrough: bool,
) -> Result<(), Unsupported> {
    let wl = surface.wl_surface();
    if passthrough {
        // A region with nothing added to it is empty, and an empty input region is the whole
        // request. `None` means the opposite — the region is infinite — so the two are not
        // interchangeable and the object has to be made.
        let region = surface.empty_region();
        wl.set_input_region(Some(&region));
        region.destroy();
    } else {
        wl.set_input_region(None);
    }
    wl.commit();
    Ok(())
}

/// Everything that has to ride the commit a frame is about to make.
///
/// Called between the frame's work being submitted and its buffer being handed over, which is the
/// one moment where all three of these are correct:
///
/// * the **frame callback**, because a callback rides the next commit and the next commit is the
///   one about to happen;
/// * the **viewport destination**, because a destination that does not match the buffer it is
///   committed with is a protocol error, and this is where the matching buffer is known; and
/// * the **presentation feedback**, because it is asked per content update and this is the update.
pub fn pre_present(surface: &WaylandSurface) {
    let logical = {
        let mut shared = surface.shared();
        shared.presented = true;
        shared.mapped = true;
        shared.pending_viewport.take()
    };
    if let Some(logical) = logical {
        surface.fractional().destination(logical);
    }
    surface.ask_for_callback();
    surface.ask_for_feedback();
    // Counted where it becomes a statement about visibility: a compositor that is not compositing
    // this surface cannot answer for its frames, and a run of unanswered ones is the only thing
    // that says so on a desktop whose shell reports neither suspension nor a leave.
    surface.shared().feedback_asked();
}

/// Begins a compositor-driven move of the surface.
///
/// This is what an application drawing its own title bar needs: a window here cannot place itself,
/// so dragging a self-drawn title bar is only possible by handing the drag to the compositor. The
/// serial quoted has to be from a press, and a press on this surface — both are refused silently.
pub fn begin_move_drag(surface: &WaylandSurface) -> Result<(), Unsupported> {
    let (seat, serial) = surface.seat().drag(zgui_platform::Surface::id(surface))?;
    let window = surface.role().window().ok_or(Unsupported)?;
    window.begin_move(&seat, serial);
    Ok(())
}

/// Begins a compositor-driven resize of the surface from `edge`.
pub fn begin_resize_drag(
    surface: &WaylandSurface,
    edge: zgui_platform::ResizeEdge,
) -> Result<(), Unsupported> {
    let (seat, serial) = surface.seat().drag(zgui_platform::Surface::id(surface))?;
    let window = surface.role().window().ok_or(Unsupported)?;
    window.begin_resize(&seat, serial, resize_edge(edge));
    Ok(())
}

/// The edge a resize was started from, in the shell's own numbering.
const fn resize_edge(edge: zgui_platform::ResizeEdge) -> ResizeEdge {
    match edge {
        zgui_platform::ResizeEdge::North => ResizeEdge::Top,
        zgui_platform::ResizeEdge::South => ResizeEdge::Bottom,
        zgui_platform::ResizeEdge::East => ResizeEdge::Right,
        zgui_platform::ResizeEdge::West => ResizeEdge::Left,
        zgui_platform::ResizeEdge::NorthEast => ResizeEdge::TopRight,
        zgui_platform::ResizeEdge::NorthWest => ResizeEdge::TopLeft,
        zgui_platform::ResizeEdge::SouthEast => ResizeEdge::BottomRight,
        _ => ResizeEdge::BottomLeft,
    }
}

#[cfg(test)]
mod tests {
    use super::resize_edge;
    use wayland_protocols::xdg::shell::client::xdg_toplevel::ResizeEdge;

    #[test]
    fn every_edge_a_resize_starts_from_crosses_to_its_own() {
        // Two edges sharing one is a window that grows the wrong way when dragged by a corner.
        let edges = [
            (zgui_platform::ResizeEdge::North, ResizeEdge::Top),
            (zgui_platform::ResizeEdge::South, ResizeEdge::Bottom),
            (zgui_platform::ResizeEdge::East, ResizeEdge::Right),
            (zgui_platform::ResizeEdge::West, ResizeEdge::Left),
            (zgui_platform::ResizeEdge::NorthEast, ResizeEdge::TopRight),
            (zgui_platform::ResizeEdge::NorthWest, ResizeEdge::TopLeft),
            (
                zgui_platform::ResizeEdge::SouthEast,
                ResizeEdge::BottomRight,
            ),
            (zgui_platform::ResizeEdge::SouthWest, ResizeEdge::BottomLeft),
        ];
        for (asked, sent) in edges {
            assert_eq!(
                resize_edge(asked),
                sent,
                "{asked:?} crossed to the wrong edge"
            );
        }
    }
}
