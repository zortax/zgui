//! This framework's own user-agent stylesheet.
//!
//! It defines the **element vocabulary**: what each element name this framework ships means before
//! any application rule touches it. That is an opinion, deliberately, and it is why no markup
//! language's defaults are inherited — a `row` is a horizontal flex container here because this
//! sheet says so, not because some other document language once said something similar.
//!
//! Three things in it are load-bearing and fail silently if they are dropped.
//!
//! **The overlay tree shape.** There is one overlay root per window; its direct children are the
//! four framework-created layer nodes, each carrying a `data-layer` attribute; portalled content
//! is always a *grandchild* of the overlay root. The `>` selectors below are written against
//! exactly that shape.
//!
//! **`position: absolute` on the layer nodes.** Stacking order applies to positioned boxes and to
//! flex and grid items only. Statically positioned layer nodes would compute all four stacking
//! values and have every one of them ignored, so cross-layer order would revert to mount order —
//! a toast raised before a modal painting underneath it.
//!
//! **`pointer-events: auto` one level below where it looks like it belongs.** Pointer events are
//! inherited, so an overlay root that refuses them makes every portalled dialog, menu and toast
//! inert. Putting `auto` on the window-spanning layer nodes overshoots the other way: a press in
//! the empty part of an open popover's layer would be swallowed by the layer instead of reaching
//! the document beneath. So the root and the four layers refuse them and only the content accepts
//! them.
//!
//! System colours arrive as custom properties rather than as a fork of the engine's device, which
//! is why the sheet refers to `--zgui-*` names it does not define: an application's theme defines
//! them, and a document with no theme installed simply has no ring colour rather than a wrong one.

/// The user-agent sheet, installed at the user-agent origin when a rule set is created.
pub const USER_AGENT_SHEET: &str = r#"
* { box-sizing: border-box; }

:root {
    display: block;
    width: 100%;
    height: 100%;
    font-family: system-ui, sans-serif;
    font-size: 16px;
    line-height: 1.5;
    color: var(--zgui-foreground);
}

box, field, control, editor  { display: block; }
/* Not one of the sixteen: the tag trait-based custom elements are built on. It carries no meaning
   of its own — the implementation decides everything — so the sheet gives it only a layout. */
custom                       { display: block; }
row                          { display: flex; flex-direction: row; }
column, stack                { display: flex; flex-direction: column; }
text, label                  { display: inline; }
image, canvas, vector, surface { display: inline-block; }
/* HTML's canvas default, because an unstyled canvas that is invisible reads as broken; an
   unstyled vector collapsing to nothing reads as "give it a size", which it should. */
canvas                       { width: 300px; height: 150px; }
scroll                       { display: block; overflow: auto; }
spacer                       { display: block; flex: 1 1 auto; }

/* The overlay root is the window, not the viewport: sized in viewport units because a fixed box's
   percentages resolve against the window less whatever gutter the page reserved for its scrollbar,
   and a sheet pinned to `right: 0` on a band one gutter short stands beside a lit strip with the
   page's scrollbar in it. Both horizontal insets are stated with the width on purpose: the leading
   edge wins the over-constraint — `left` in a left-to-right document, `right` in a right-to-left
   one — and the leading edge is exactly the one a vertical scrollbar is not on, so the root starts
   flush against the window in either direction and grows across the strip. */
overlay_root                        { display: block; position: fixed; left: 0; right: 0; top: 0;
                                      width: 100vw; height: 100vh; pointer-events: none; }
overlay_root > [data-layer]         { position: absolute; inset: 0; pointer-events: none; }
overlay_root > [data-layer] > *     { pointer-events: auto; }
overlay_root > [data-layer=content] { z-index: 10; }
overlay_root > [data-layer=popover] { z-index: 20; }
overlay_root > [data-layer=modal]   { z-index: 30; }
overlay_root > [data-layer=toast]   { z-index: 40; }

:focus-visible { outline: 2px solid var(--zgui-ring); outline-offset: 2px; }
:disabled      { pointer-events: none; }
::selection    { background-color: var(--zgui-selection); color: var(--zgui-selection-text); }
[hidden]       { display: none; }
"#;

/// The scrollbar metrics the scroll element reserves space with, in CSS pixels.
///
/// Not a declaration in the sheet above, because there is no CSS property that states it: the
/// engine's device answers "how wide is a classic scrollbar" from a fixed number, and this is the
/// number this framework uses when it lays a scroll container out.
pub const SCROLLBAR_SIZE: f32 = 15.0;
