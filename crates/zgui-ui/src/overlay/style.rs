//! What every floating surface shares, in tokens.

use zgui::style;

style! { pub OverlayStyle =>
    // The scrim behind a modal surface. It covers the whole window, scrollbar gutter included,
    // because the overlay bands it is placed on are themselves sized to the window in viewport
    // units — `100vw`/`100vh` in the user-agent sheet — rather than to the viewport a fixed box's
    // percentages resolve against, which is the window less whatever gutter the page reserved for
    // its scrollbar. A scrim one gutter short leaves a lit strip fifteen pixels wide down the right
    // of the screen, and a modal surface that dims all of the interface except the scrollbar
    // beside it has not dimmed the interface.
    ":scope {
        position: fixed;
        inset: 0;
        background-color: var(--zui-color-scrim);
        animation: zui-scrim-enter var(--zui-motion-duration-normal) ease both;
    }"
    // A scrim on its way out takes no pointer. The press that closed a surface is often followed
    // within the exit's own duration by the press the user actually meant — a click on the page a
    // moment after Escape — and a fading scrim that still swallowed it would leave the page dead
    // for exactly as long as the animation runs.
    ":scope[data-state=\"closed\"] {
        animation: zui-scrim-exit var(--zui-motion-duration-normal) ease both;
        pointer-events: none;
    }"
    "@keyframes zui-scrim-enter { from { opacity: 0; } to { opacity: 1; } }"
    "@keyframes zui-scrim-exit { from { opacity: 1; } to { opacity: 0; } }"

    // The surface itself: the panel a popover, a menu, a hover card and a dialog all are. The
    // resting look is a popover's, because that is what most of them are.
    //
    // Each of the five is read out of a custom property rather than written outright, and a
    // surface that differs sets the property rather than restating the declaration. That is not
    // ceremony: a rule stating `background-color` here and a rule stating it in the dialog's own
    // sheet are the same weight, so which one wins would come down to which sheet reached the
    // cascade first — and a dialog's sheet is installed *before* the shared one it goes on to
    // build, so the shared answer would win and every dialog would be a popover's colour. Setting
    // a property cannot contend with reading one, and the surfaces that out-specify these on
    // purpose — a test that paints two panels apart, an application restyling one — keep working
    // against a single class rather than having to match two.
    ".zui-surface {
        display: flex;
        flex-direction: column;
        border: var(--zui-surface-border, 1px solid var(--zui-color-border));
        border-radius: var(--zui-surface-radius, var(--zui-radius-md));
        background-color: var(--zui-surface-fill, var(--zui-color-popover));
        color: var(--zui-surface-ink, var(--zui-color-popover-foreground));
        box-shadow: var(--zui-surface-shadow, var(--zui-shadow-md));
        transform-origin: var(--zui-surface-origin-x, center) var(--zui-surface-origin-y, center);
    }"

    // # How a surface enters and leaves
    //
    // Two keyframes for the whole library, and everything that varies between one surface and the
    // next is a custom property either of them reads. A dialog wants a longer fade than a menu; a
    // sheet slides a whole panel width and takes different times coming and going; a tooltip does
    // not slide at all. None of that is a second pair of keyframes — it is six numbers.
    //
    // Keyframes rather than transitions, and that is not a preference. A transition only runs when
    // a property *changes* on an element that was already laid out, and a surface does not exist
    // until the frame it opens on: there is no earlier value to move away from, so the entrance
    // would simply not happen. An animation states both ends and needs no history.
    //
    // Where the surface sits is composed in rather than overwritten. A surface has two independent
    // reasons to carry a transform — the placement it owns, which for a dialog is the half of its
    // own size it pulls itself back by, and the entrance every surface shares — and a keyframe
    // that wrote `transform` outright would drop the first, hanging the dialog below and right of
    // the centre by half its size for as long as the animation ran.
    ".zui-surface {
        animation: zui-surface-enter var(--zui-surface-enter-duration, var(--zui-motion-duration-normal))
            var(--zui-surface-enter-ease, ease) both;
    }"
    // Like the scrim: a leaving surface is a picture, not a control, and the pointer belongs to
    // whatever is under it for the rest of its exit.
    ".zui-surface[data-state=\"closed\"] {
        animation: zui-surface-exit var(--zui-surface-exit-duration, var(--zui-motion-duration-normal))
            var(--zui-surface-exit-ease, ease) both;
        pointer-events: none;
    }"
    "@keyframes zui-surface-enter {
        from {
            opacity: var(--zui-surface-enter-opacity, 0);
            transform: var(--zui-surface-place, translate(0px, 0px))
                translate(var(--zui-surface-enter-x, 0px), var(--zui-surface-enter-y, 0px))
                scale(var(--zui-surface-enter-scale, 0.95));
        }
        to {
            opacity: 1;
            transform: var(--zui-surface-place, translate(0px, 0px)) translate(0px, 0px) scale(1);
        }
    }"
    "@keyframes zui-surface-exit {
        from {
            opacity: 1;
            transform: var(--zui-surface-place, translate(0px, 0px)) translate(0px, 0px) scale(1);
        }
        to {
            opacity: var(--zui-surface-exit-opacity, 0);
            transform: var(--zui-surface-place, translate(0px, 0px))
                translate(var(--zui-surface-exit-x, 0px), var(--zui-surface-exit-y, 0px))
                scale(var(--zui-surface-exit-scale, 0.95));
        }
    }"

    // Which way an anchored surface comes in from is the side the positioner actually chose, which
    // may not be the side it asked for. Selecting on the positioner's own attribute is what keeps
    // the two in step without a line of Rust. It slides *towards* its trigger — a surface below the
    // trigger starts eight pixels high and settles down onto its place.
    //
    // A descendant selector rather than a child one, because what is between the positioner and the
    // surface depends on the surface: a popover confines focus and so has a scope element in the
    // way, and a tooltip, which must not, has nothing. Nothing else can match, because a surface
    // opened from inside this one is portalled onto its own band rather than nested here.
    ".zui-overlay-positioner[data-side=\"bottom\"] .zui-surface { --zui-surface-enter-y: -8px; }"
    ".zui-overlay-positioner[data-side=\"top\"] .zui-surface { --zui-surface-enter-y: 8px; }"
    ".zui-overlay-positioner[data-side=\"left\"] .zui-surface { --zui-surface-enter-x: 8px; }"
    ".zui-overlay-positioner[data-side=\"right\"] .zui-surface { --zui-surface-enter-x: -8px; }"

    // And what it grows *out of* is the corner nearest that trigger, so the zoom reads as the
    // surface unfolding from the control rather than swelling out of its own middle. The side
    // settles one axis and the alignment settles the other, which is why the alignment rules name
    // the side as well: for a surface below its trigger the alignment runs left to right, and for
    // one beside it the same word means top to bottom.
    ".zui-overlay-positioner[data-side=\"bottom\"] .zui-surface { --zui-surface-origin-y: top; }"
    ".zui-overlay-positioner[data-side=\"top\"] .zui-surface { --zui-surface-origin-y: bottom; }"
    ".zui-overlay-positioner[data-side=\"left\"] .zui-surface { --zui-surface-origin-x: right; }"
    ".zui-overlay-positioner[data-side=\"right\"] .zui-surface { --zui-surface-origin-x: left; }"
    ".zui-overlay-positioner[data-side=\"top\"][data-align=\"start\"] .zui-surface,
     .zui-overlay-positioner[data-side=\"bottom\"][data-align=\"start\"] .zui-surface {
        --zui-surface-origin-x: left;
    }"
    ".zui-overlay-positioner[data-side=\"top\"][data-align=\"end\"] .zui-surface,
     .zui-overlay-positioner[data-side=\"bottom\"][data-align=\"end\"] .zui-surface {
        --zui-surface-origin-x: right;
    }"
    ".zui-overlay-positioner[data-side=\"left\"][data-align=\"start\"] .zui-surface,
     .zui-overlay-positioner[data-side=\"right\"][data-align=\"start\"] .zui-surface {
        --zui-surface-origin-y: top;
    }"
    ".zui-overlay-positioner[data-side=\"left\"][data-align=\"end\"] .zui-surface,
     .zui-overlay-positioner[data-side=\"right\"][data-align=\"end\"] .zui-surface {
        --zui-surface-origin-y: bottom;
    }"

    // A dismissable layer, a focus scope and the box that carries a surface's depth are behaviours
    // rather than boxes. They still build an element, so they are told to get out of the layout's
    // way — and a custom property set on one of them still inherits to everything below it, which
    // is how the depth reaches the boxes that stack.
    ".zui-overlay-layer, .zui-overlay-scope, .zui-overlay-depth { display: contents; }"

    // How deep a surface is on its band *is* its stacking order there, and it has to be stated
    // rather than left to the order the boxes arrived in: a surface opened from inside another is
    // built while that one's content is, so it reaches the band first and would be painted first
    // — underneath the surface that opened it. The three selectors are the three boxes a band ever
    // stacks: the scrim behind a modal surface, the positioner around an anchored one, and a modal
    // surface's own panel, which positions itself.
    ".zui-overlay-scrim, .zui-overlay-positioner, .zui-surface {
        z-index: var(--zui-overlay-depth, 0);
    }"
}
