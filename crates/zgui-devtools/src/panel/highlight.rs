//! The rectangle drawn over whatever the inspector is pointing at.
//!
//! Portalled onto the topmost overlay layer, so it is over the application rather than inside it:
//! the thing being outlined is frequently inside something that clips, and an outline drawn as a
//! child of what it outlines would be cut off by exactly the box a reader is trying to see the
//! edges of.
//!
//! **It is mounted for as long as the panel is open and hidden with `display` rather than being
//! added and removed.** That is what keeps the tree tab still: the tree is a sample of the
//! document, hovering a row changes what is outlined, and an outline that mounted a node would
//! change the document — so every hover would invalidate the sample, which would redraw the rows,
//! which is the pointer moving over a row that has just been rebuilt underneath it. Hidden and
//! shown, the document's shape is the same whatever the pointer is doing and only four inline
//! lengths move.
//!
//! It takes no pointer events. Picking works by asking what is under the pointer, and a rectangle
//! covering the thing being picked would be the answer to that question every time.

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, on_cleanup_local};
use zgui::view::NodeRef;
use zgui::{component, view};

use crate::state::DevTools;

/// The outline over the application.
#[component]
pub(crate) fn HighlightOverlay(
    /// Where the rectangle to draw is published.
    tools: DevTools,
) -> impl IntoView {
    let outline = tools.highlight_box;
    // Published for the sampler, which has to leave it out of the tree: it is the inspector's own,
    // and a tree containing the outline drawn over what the tree is pointing at would be a tree
    // that grew every time somebody hovered a row.
    let drawn = NodeRef::new();
    let publish = RenderEffect::new(move |_: Option<()>| {
        let node = drawn.get();
        if tools.overlay.get_untracked() != node {
            tools.overlay.set(node);
        }
    });
    on_cleanup_local(move || drop(publish));

    view! {
        Portal(layer = {OverlayLayer::Toast}) {
            box(
                class = "zgui-devtools-highlight",
                node_ref = drawn,
                style:display = move || {
                    Some(if outline.get().is_some() { "flex" } else { "none" }.to_owned())
                },
                style:left = move || outline.get().map(|at| format!("{:.1}px", at.origin.x.0)),
                style:top = move || outline.get().map(|at| format!("{:.1}px", at.origin.y.0)),
                style:width = move || outline.get().map(|at| format!("{:.1}px", at.size.width.0)),
                style:height = move || outline.get().map(|at| format!("{:.1}px", at.size.height.0))
            )
        }
    }
}
