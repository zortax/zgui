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
/// A compositor that disagrees says so with the next configure, which wins. A request made while
/// the window is maximised or full screen does not survive a restore: the saved normal extent
/// replaces it.
///
/// A shell layer may name zero along an axis, which leaves that axis to the compositor: a bar
/// across the screen changes its height alone this way. It is drawn at its current extent along
/// that axis until the compositor answers.
pub fn request_size(
    surface: &WaylandSurface,
    size: Size<CssPx, Css>,
) -> Option<Size<DevicePx, Device>> {
    // A lock surface has the extent of its output, and a buffer of any other extent is a
    // protocol error.
    if surface.role().lock().is_some() {
        return None;
    }
    let layer = surface.role().layer().is_some();
    let refused = |extent: CssPx| extent.0 < 0.0 || (!layer && extent.0 == 0.0);
    if refused(size.width) || refused(size.height) || (size.width.0 == 0.0 && size.height.0 == 0.0)
    {
        return None;
    }
    surface.set_wanted(size);
    let (scale, size) = {
        let mut shared = surface.shared();
        let scale = shared.scale;
        let size = Size::new(
            if size.width.0 > 0.0 {
                size.width
            } else {
                shared.logical.width
            },
            if size.height.0 > 0.0 {
                size.height
            } else {
                shared.logical.height
            },
        );
        let resized = shared.resized(size, scale);
        shared.unreported = resized;
        // A shell layer states its extent with the frame drawn at it, so the two change together.
        shared.layer_owed |= shared.layer.is_some();
        (scale, size)
    };
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
///
/// An unmapped surface is shown again by the same empty commit that asked for its first configure.
/// The pacer holds the redraw until a configure that answers that commit arrives. A shell layer is
/// a fresh surface to the shell after an unmap, so its whole placement and extent go out again
/// before that commit.
///
/// A surface hidden before the first buffer of its mapping is still in that mapping's configure
/// sequence. Hiding it commits nothing, and showing it again only lets the next frame draw once
/// that configure is in.
///
/// A hide asked for while the surface draws waits for the end of that delivery, because the frame
/// being drawn commits its buffer after the request. Until the surface is shown again it commits
/// nothing: an empty commit on an unmapped surface asks the shell to map it.
///
/// A lock surface stays shown. A lock surface with no buffer is a protocol error.
pub fn set_visible(surface: &WaylandSurface, visible: bool) {
    if !visible && surface.role().lock().is_some() {
        return;
    }
    if visible {
        let remap = {
            let mut shared = surface.shared();
            shared.hide_owed = false;
            let unmapped = std::mem::take(&mut shared.unmapped);
            let remap = unmapped && shared.sequence.needs_initial_commit();
            if unmapped && !remap {
                let configured = std::mem::take(&mut shared.configured_unshown);
                shared.visibility.configured |= configured;
            }
            if remap {
                shared.sequence.restarted();
            }
            remap
        };
        if remap {
            surface.send_layer(true);
            surface.send_regions();
            surface.wl_surface().commit();
        }
        zgui_platform::Surface::request_redraw(surface);
        return;
    }
    {
        let mut shared = surface.shared();
        if shared.unmapped {
            return;
        }
        if shared.drawing {
            shared.hide_owed = true;
            return;
        }
    }
    unmap(surface);
}

/// Commits no buffer, which unmaps the surface, and forgets what the compositor owed it.
///
/// A surface that shows no buffer is not mapped, so it commits nothing. It keeps the configure it
/// holds for the show. A mapped surface sends a barrier right after the commit, and every
/// configure that arrives before the answer is dropped unacknowledged: the shell sent it for the
/// mapping that is gone.
pub(crate) fn unmap(surface: &WaylandSurface) {
    let shown = surface.shared().shown;
    if shown {
        let wl = surface.wl_surface();
        wl.attach(None, 0, 0);
        wl.commit();
        let serial = surface.shared().sequence.reset();
        surface.send_barrier(serial);
    }
    let mut shared = surface.shared();
    shared.hide_owed = false;
    shared.shown = false;
    if !shown {
        shared.configured_unshown |= shared.visibility.configured;
    }
    shared.visibility.configured = false;
    shared.unmapped = true;
    // A frame held for the compositor would map the surface again when it is presented.
    shared.pacer.cancel_held();
    shared.cancel_callbacks();
}

/// Makes the surface transparent to the pointer, or takes it back.
///
/// An empty input region is the protocol's way of saying "nothing here is mine". It is committed
/// at once rather than with the next frame: a region that waits for a frame would let through the
/// very click it was set for. It replaces an input region asked for and not yet sent. A hidden
/// surface commits nothing, because an empty commit asks the shell to map it; the region then
/// goes out with the commit that shows the surface again. A lock surface that owes a buffer at a
/// new extent commits nothing either; the region goes out with that buffer.
pub fn set_pointer_passthrough(
    surface: &WaylandSurface,
    passthrough: bool,
) -> Result<(), Unsupported> {
    let bare = {
        let mut shared = surface.shared();
        shared.input_owed = None;
        shared.may_commit_bare()
    };
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
    if bare {
        wl.commit();
    }
    Ok(())
}

/// Everything that has to ride the commit a frame of `size` at `scale` is about to make.
///
/// Called between the frame's work being submitted and its buffer being handed over, which is the
/// one moment where all of these are correct:
///
/// * the **frame callback**, because a callback rides the next commit and the next commit is the
///   one about to happen;
/// * the **buffer scale, viewport destination and window geometry**, because each describes the
///   buffer it is committed with — a destination or a scale that does not match it is a protocol
///   error — and the frame's own extent is known here, even when a newer configure has already
///   arrived; and
/// * the **presentation feedback**, because it is asked per content update and this is the update.
pub fn pre_present(surface: &WaylandSurface, size: Size<i32, Device>, scale: f64) {
    let (metadata, fractional) = {
        let mut shared = surface.shared();
        shared.presented = true;
        shared.mapped = true;
        shared.shown = true;
        shared.committing_frame(size);
        (
            shared.frame_metadata(size, scale),
            shared.ladder.is_fractional(),
        )
    };
    if let Some(logical) = metadata {
        // A surface with a viewport is always mapped through it: the buffer can be larger than the
        // frame, and only the frame's extent is shown.
        let viewport = surface.fractional().crops();
        crate::surface::scale::declare(surface, scale, fractional || viewport);
        surface.fractional().source(size);
        surface.fractional().destination(logical);
        if let Some(window) = surface.role().window() {
            window.set_geometry(logical);
        }
    }
    surface.send_layer(false);
    surface.send_regions();
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
