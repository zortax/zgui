//! A floating surface, anchored to something and kept inside the window.

mod placement;
mod scale;
mod solve;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal};
use zgui::{component, view};

pub use crate::popper::placement::{Align, Placement, Side};
use crate::popper::scale::Density;
pub use crate::popper::solve::{PopperOptions, Solution, WindowRect, solve};

/// Places its children against an anchor, and keeps them inside the window.
///
/// Every floating surface there is — a popover, a menu, a tooltip, a select's list — needs the
/// same three things, and none of them can be decided without measuring: which side of the anchor
/// there is room on, how far along the anchor's edge the surface has to slide to stay on screen,
/// and where the anchor is now that something has scrolled.
///
/// # It renders its own positioner
///
/// The surface is placed by an element this component creates and owns, wrapped around the
/// children. It cannot be otherwise: a view may only write on nodes it made, so a component handed
/// somebody else's handle would have no way to move it.
///
/// # It is placed in the frame it opens
///
/// A surface that appeared in the wrong corner for one frame appeared in the wrong corner, and no
/// later correction takes that frame back. So the positioner mounts hidden, its measured size and
/// the anchor's box arrive together during the same frame, and the offset is written and the
/// visibility cleared before anything is painted. One measurement pass, and the surface is visible
/// and correct the first time it is drawn.
///
/// # A surface that is not showing watches nothing
///
/// Placement is made from three live measurements, and a live measurement costs the frame that
/// delivers it. A surface that is unmounted while it is closed — which is what [`Presence`](crate::presence::Presence) gives
/// it — stops watching by ceasing to exist. A surface that stays mounted while it is closed,
/// because something outside it names its element, says so with `active`: while that answers
/// false, no measurement is watched, no placement is computed and the positioner is hidden.
///
/// # What it tells the style sheet
///
/// `data-side` and `data-align` carry where the surface actually went, which may not be where it
/// was asked to go. An arrow points from those, and an entry animation slides from the right
/// direction, with no Rust involved:
///
/// ```text
/// .popover[data-side="top"]    { animation: slide-up 180ms }
/// .popover[data-side="bottom"] { animation: slide-down 180ms }
/// ```
///
/// # Two kinds of pixel
///
/// Every measurement a placement is made from arrives in device pixels, so that is the space the
/// placement is solved in and the space the offsets given here are converted into. The answer is
/// written back as an inline `left` and `top`, and a length in a style sheet is a CSS pixel — so
/// the last thing that happens to a placement is a division by the surface's density. On a display
/// of one device pixel per CSS pixel the two spaces coincide and the division changes nothing; on a
/// denser one, skipping it multiplies the surface's position by that density and puts it past
/// whatever it was placed against.
///
/// # Where to put it
///
/// Inside a [`Portal`], so the surface escapes whatever clipped or transformed ancestor the
/// trigger happens to live in. The positioner is placed in the window's own pixels, so it works
/// wherever it is mounted; the portal is about clipping and stacking, not about coordinates.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui_primitives::prelude::*;
///
/// #[component]
/// fn Tooltip() -> impl IntoView {
///     let trigger = NodeRef::new();
///     view! {
///         box {
///             control(node_ref = trigger) {"hover me"}
///             Portal {
///                 Popper(anchor = trigger, placement = Placement::TOP) {
///                     surface(class = "tooltip") {"Bold"}
///                 }
///             }
///         }
///     }
/// }
/// ```
#[component]
#[allow(clippy::too_many_arguments)]
pub fn Popper(
    /// What the surface is placed against.
    anchor: NodeRef,
    /// Where it is asked to go.
    #[prop(into, default = Signal::stored_local(Placement::BOTTOM))]
    placement: Signal<Placement, LocalStorage>,
    /// Whether to cross to the other side of the anchor when there is not enough room.
    #[prop(default = true)]
    flip: bool,
    /// Whether to slide along the anchor's edge to stay inside the window.
    #[prop(default = true)]
    shift: bool,
    /// How far off the anchor the surface sits, in CSS pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// How close to the window's edge the surface may come, in CSS pixels.
    #[prop(default = 8.0)]
    padding: f32,
    /// Whether the surface is on screen and therefore worth placing.
    ///
    /// A surface that is not showing is not placed, and — the part that costs something — it
    /// watches nothing while it is not showing. The three measurements a placement is made from
    /// are delivered by the frame, and the anchor's box is reported in the window's own pixels, so
    /// a surface that keeps watching while it is closed is re-placed on every frame in which
    /// anything scrolls. Most callers keep the surface unmounted while it is closed and want the
    /// default; a caller whose surface is always mounted — because something else names its
    /// element — says so here.
    #[prop(into, default = Signal::stored_local(true))]
    active: Signal<bool, LocalStorage>,
    /// Where to record the positioner, for a caller that has to measure it.
    #[prop(optional)]
    element_ref: Option<NodeRef>,
    /// Extra classes on the positioner.
    #[prop(into, optional)]
    class: Classes,
    /// The surface.
    children: Children,
) -> impl IntoView {
    let positioner = element_ref.unwrap_or_default();

    // Three measurements, all of them live. The anchor's box moves when anything scrolls or the
    // layout changes; the surface's own size is exactly what the decision changes, so it cannot
    // come from a previous frame; and the window's own box is what "inside" means.
    let anchor_box = anchor.observe_border_box_while(move || active.get());
    let floating_size = positioner.observe_content_size_while(move || active.get());

    // The window's own box, observed rather than read: a resize moves every edge a surface is
    // being kept inside of, and a one-shot read would place the last-opened menu correctly and
    // every later one against a window that has changed size.
    //
    // It is acquired once, from an effect, because the window's root is only reachable through a
    // handle that is bound — and this component's own handle binds as its element is built.
    let window_box: RwSignal<Option<Signal<Option<WindowRect>, LocalStorage>>, LocalStorage> =
        RwSignal::new_local(None);
    let watching_window = RenderEffect::new(move |_| {
        if positioner.get().is_none() || window_box.get_untracked().is_some() {
            return;
        }
        if let Some(root) = positioner.window_root() {
            window_box.set(Some(root.observe_border_box_while(move || active.get())));
        }
    });
    on_cleanup_local(move || drop(watching_window));
    let viewport =
        Signal::derive_local(move || window_box.get().and_then(|observed| observed.get()));

    let solution = Signal::derive_local(move || {
        // A surface that is not showing has no placement, and asking for one would be asking for
        // the measurements it has deliberately stopped watching.
        if !active.get() {
            return None;
        }
        let anchor = anchor_box.get()?;
        let floating = floating_size.get();
        // Before the first measurement the surface has no size, and a solution computed from
        // nothing would place it against the anchor's corner and then move it — which is the one
        // frame in the wrong place this whole design exists to remove.
        if floating.width.0 <= 0.0 && floating.height.0 <= 0.0 {
            return None;
        }
        let viewport = viewport.get()?;
        // The three measurements are in device pixels, so the two lengths the caller stated in CSS
        // pixels are converted into that space rather than the other three out of it.
        let density = Density::reported(positioner.scale());
        Some(solve(
            anchor,
            floating,
            viewport,
            &PopperOptions {
                placement: placement.get(),
                flip,
                shift,
                offset: density.device(offset),
                padding: density.device(padding),
            },
        ))
    });

    // Back into CSS pixels, because that is what an inline length is read as. The solution itself
    // stays in the space its measurements came in, so `data-side` and everything downstream of it
    // is unaffected by which display the window is on.
    //
    // Snapped to a whole device pixel on the way, for two reasons. The visible one is that a
    // surface whose edge falls between two pixels is a surface with a soft border and blurred text
    // on one side only. The other is that this is a loop: the origin is written as `left` and
    // `top` on a fixed-position box, and the room left beside those edges is what the box's own
    // shrink-to-fit width is measured against — so a fractional origin can produce a width that
    // produces a different fractional origin, and the two chase each other for as long as the
    // surface is open. Whole pixels have nowhere to chase to.
    let origin = Signal::derive_local(move || {
        let solved = solution.get()?;
        let density = Density::reported(positioner.scale());
        Some((
            density.css(solved.origin.x.0.round()),
            density.css(solved.origin.y.0.round()),
        ))
    });
    let left = move || origin.get().map(|(x, _)| px(x));
    let top = move || origin.get().map(|(_, y)| px(y));
    // Hidden rather than absent: an element that is not there has no size to measure, and the
    // measurement is what decides where it goes. `visibility` keeps the box and its layout and
    // takes it out of the paint, which is exactly the state a surface being placed is in.
    let visibility = move || solution.get().is_none().then(|| "hidden".to_owned());
    let side = move || {
        solution
            .get()
            .map(|solved| solved.placement.side.name().to_owned())
    };
    let align = move || {
        solution
            .get()
            .map(|solved| solved.placement.align.name().to_owned())
    };

    view! {
        box(
            class = class,
            node_ref = positioner,
            style:position = "fixed",
            style:left = left,
            style:top = top,
            style:visibility = visibility,
            attr:data-side = side,
            attr:data-align = align
        ) {
            {children.into_view_once()}
        }
    }
}

/// A length as a CSS pixel value.
fn px(value: f32) -> String {
    format!("{value}px")
}

#[cfg(test)]
mod tests {
    use super::px;

    #[test]
    fn a_length_is_written_as_a_css_pixel_value() {
        assert_eq!(px(0.0), "0px");
        assert_eq!(px(-12.5), "-12.5px");
    }
}
