//! What a change of device pixel ratio invalidates.
//!
//! A window's ratio is not a constant. It changes when the window is dragged onto another monitor,
//! when the desktop's scaling is changed while the application is running, and on the very first
//! configure a Wayland compositor sends. Everything the framework measures in device pixels is
//! wrong from that moment, and everything it measures in CSS pixels is untouched — so the change
//! is a *boundary*, and this module is where it is drawn.
//!
//! | State | Unit | What the change does to it |
//! |---|---|---|
//! | the cascade's computed styles | CSS px | nothing: `font-size: 12px` is twelve CSS pixels at every ratio |
//! | the layout cache | **device px** | invalidated in full — see below |
//! | shaped paragraphs | device px | keyed by the ratio they were shaped at, so a new ratio misses and re-shapes |
//! | rasterised glyphs | device px | keyed by their size in device pixels, so a new ratio misses and re-rasterises |
//! | scroll offsets | **device px** | multiplied by the change, so the reader stays at the same place in the document |
//! | the media device | ratio | rebuilt, because `resolution` and `dppx` queries read it |
//!
//! # Why the layout cache has to be emptied by hand
//!
//! The other three answer for themselves. A shaped paragraph and a rasterised glyph are looked up
//! under keys that carry the ratio, so a window at a new one asks a question nothing has answered
//! and the answer is made afresh. The layout cache is not like that: it is keyed by the *question*
//! the layout algorithms asked — a run mode, an available space, a known size — and every one of
//! those is a number in device pixels. At a new ratio the same box asks a differently-sized
//! question and would ordinarily miss; but a container asking for a min-content width, or any box
//! whose result was reached without a size coming in, asks a question that is identical at every
//! ratio, and is answered from a slot computed at the old one.
//!
//! The symptom is a document that half rescales. A box with an explicit `width` moves and grows
//! correctly, because its style is converted afresh; a box sized by its own text keeps the extent
//! its text had at the previous ratio. On the gallery that is a badge that stays the width of
//! one-times text inside two-times padding, and a masthead of the wrong height beneath which every
//! other panel sits in the wrong place.
//!
//! Nothing narrower than the whole tree is correct, because a scale change makes every length in
//! every box wrong at once.

use zgui_geom::CssPx;
use zgui_style::Viewport;

use crate::window::Window;

impl Window {
    /// Moves the window to a new device pixel ratio over a surface of `width` by `height` device
    /// pixels, and reports whether the ratio actually moved.
    ///
    /// The viewport it publishes to the cascade stays in CSS pixels — a `@media (width: …)` query
    /// is written in them, and `Viewport::scale` is what a `dppx` query reads.
    pub(crate) fn rescale(&mut self, scale: f32, width: f32, height: f32) -> bool {
        let moved = scale != self.scale;
        let from = self.scale;
        self.scale = scale;
        // Published to the view seam as well: a component relating a pointer position to an
        // element's box needs the number that converts CSS pixels to device ones, and a surface
        // that moved to another monitor has changed it.
        self.host.set_scale(scale);
        // The colour scheme is carried across rather than defaulted. It is a property of the
        // desktop and not of the surface's extent, so a window that is resized, or dragged onto
        // another monitor, must not be re-styled as though the user had just asked for light.
        self.viewport = Viewport::new(CssPx(width / scale), CssPx(height / scale))
            .at_scale(scale)
            .in_scheme(self.color_scheme());
        if moved {
            let marked = zgui_layout::tree::dirty::mark_all_dirty(&mut self.layout.borrow_mut());
            // A scroll offset is a number of *device* pixels, so it is one of the things the new
            // ratio has made wrong — and it is the one thing in the table above that no cache miss
            // can correct, because nothing looks it up under a key at all. Carried across
            // unchanged it stands for a different place in the document: a reader at the bottom of
            // a page on a one-times output arrives halfway down it on a two-times one, and coming
            // back the other way arrives at an offset the smaller surface has no content for.
            self.rescale_scroll(scale / from);
            zgui_profile::latency::note_with("w.rescale", || {
                format!("scale={scale} boxes={marked}")
            });
        }
        moved
    }
}
