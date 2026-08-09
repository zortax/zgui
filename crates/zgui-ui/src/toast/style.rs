//! What a stack of toasts looks like, in tokens.

use zgui::style;

style! { pub ToastStyle =>
    ".zui-toast__host { display: contents; }"
    // The region is pinned to its corner and has no size of its own: every toast is placed against
    // that corner absolutely, so the region never has to be as tall as the stack and a toast that
    // leaves takes its box away without moving anything in the layout.
    //
    // `--zui-toast-move` is the one duration everything that *moves* a toast reads: its arrival,
    // its departure, the step it takes when the toast under it goes, and the fade between a
    // collapsed card and an expanded one. It is written out rather than taken from the motion
    // ladder because a stack of messages sliding in from off the screen is a longer distance than
    // any of the ladder's steps was chosen for, and a stack that arrives at a menu's pace reads as
    // a flicker. Colour changes inside a toast are not this — they stay on the ladder.
    ":scope {
        --zui-toast-width: 356px;
        --zui-toast-gap: 14px;
        --zui-toast-move: 400ms;
        position: fixed;
        width: var(--zui-toast-width);
        max-width: 100%;
        /* The stack's whole outline, computed by the queue: the slots are absolutely placed, so
           without this the region is a line of no height that no pointer can enter — and the
           region is what holds the stack open, gaps included. */
        height: var(--zui-toast-extent, 0px);
        margin: calc(var(--zui-space-base) * 6);
        pointer-events: auto;
    }"
    // How far the stack is from the window's edge is a margin rather than an inset, because an inset
    // written in a token is a declaration a document with no theme installed drops altogether — and
    // what it would drop is the half that says which corner this is.
    ":scope[data-corner=\"bottom-right\"] { right: 0; bottom: 0; }"
    ":scope[data-corner=\"bottom-left\"] { left: 0; bottom: 0; }"
    ":scope[data-corner=\"top-right\"] { right: 0; top: 0; }"
    ":scope[data-corner=\"top-left\"] { left: 0; top: 0; }"
    // One slot per toast. Three things move it: how far it is from the corner, how far down the deck
    // it is while the stack is collapsed, and how far a finger has pushed it. All are custom
    // properties feeding one transform, so none of them costs a layout and all of them can be
    // transitioned.
    //
    // The slot takes no pointer of its own: the hold that keeps the stack open is the region's,
    // whose box covers toasts and gaps alike. A slot that took it would stand between the pointer
    // and the toast underneath — the slot's gap padding lies over the neighbouring toast, exactly
    // where that toast's close control is pressed.
    //
    // The stacking number is written by the item, highest at the front, because the region keeps
    // its children newest first and document order alone would paint the oldest toast on top.
    ".zui-toast__slot {
        position: absolute;
        left: 0;
        width: 100%;
        display: flex;
        flex-direction: column;
        pointer-events: none;
        z-index: var(--zui-toast-layer, 0);
        transition: transform var(--zui-toast-move) var(--zui-motion-ease-standard);
    }"
    // The gap between two toasts is padding on the slot, on the side the next toast is on, so the
    // step from one to the next is exactly what the slot measured and no number is written twice.
    ":scope[data-corner=\"bottom-right\"] .zui-toast__slot,
     :scope[data-corner=\"bottom-left\"] .zui-toast__slot {
        bottom: 0;
        padding-top: var(--zui-toast-gap);
        transform: translateX(var(--zui-toast-swipe, 0px))
                   translateY(calc(-1 * var(--zui-toast-offset, 0px)));
    }"
    ":scope[data-corner=\"top-right\"] .zui-toast__slot,
     :scope[data-corner=\"top-left\"] .zui-toast__slot {
        top: 0;
        padding-bottom: var(--zui-toast-gap);
        transform: translateX(var(--zui-toast-swipe, 0px))
                   translateY(var(--zui-toast-offset, 0px));
    }"
    // Collapsed: a deck rather than a list. Each toast behind the front one steps by the gap and
    // shrinks by a twentieth, so what shows is the edge of the one under it. Expanding is the same
    // transform going back to the measured offsets, which is why one transition covers both.
    ":scope[data-expanded=\"false\"][data-corner=\"bottom-right\"] .zui-toast__slot,
     :scope[data-expanded=\"false\"][data-corner=\"bottom-left\"] .zui-toast__slot {
        transform: translateX(var(--zui-toast-swipe, 0px))
                   translateY(calc(-1 * var(--zui-toast-gap) * var(--zui-toast-depth, 0)))
                   scale(calc(1 - 0.05 * var(--zui-toast-depth, 0)));
        transform-origin: bottom center;
    }"
    ":scope[data-expanded=\"false\"][data-corner=\"top-right\"] .zui-toast__slot,
     :scope[data-expanded=\"false\"][data-corner=\"top-left\"] .zui-toast__slot {
        transform: translateX(var(--zui-toast-swipe, 0px))
                   translateY(calc(var(--zui-toast-gap) * var(--zui-toast-depth, 0)))
                   scale(calc(1 - 0.05 * var(--zui-toast-depth, 0)));
        transform-origin: top center;
    }"
    // Only the front toast of a collapsed deck says anything. The ones under it are shapes, and a
    // deck of three messages all legible at once is three messages nobody reads.
    ":scope[data-expanded=\"false\"] .zui-toast__slot[data-front=\"false\"] .zui-toast > * {
        opacity: 0;
    }"
    ".zui-toast > * { transition: opacity var(--zui-toast-move) var(--zui-motion-ease-standard); }"
    // While a finger is on it the toast follows without easing: a transition here would make the
    // toast trail behind the pointer, which reads as lag rather than as a gesture.
    ".zui-toast__slot[data-swiping=\"true\"] { transition: none; }"
    // The toast itself carries its entrance and its exit, and no transition. A transition and a
    // keyframe animation over one property are two authorities over one value.
    ".zui-toast {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: calc(var(--zui-space-base) * 1.5);
        padding: var(--zui-space-lg);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-lg);
        background-color: var(--zui-color-popover);
        color: var(--zui-color-popover-foreground);
        box-shadow: 0 4px 12px rgb(0 0 0 / 0.1);
        font-family: var(--zui-type-family-sans);
        font-size: 13px;
        width: 100%;
        box-sizing: border-box;
        position: relative;
        pointer-events: auto;
        animation: zui-toast-in var(--zui-toast-move) var(--zui-motion-ease-out);
    }"
    // Held at its last keyframe, because the row is not taken off the stack until the animation has
    // ended and a toast that snapped back to opaque in between would flash on its way out.
    ".zui-toast[data-state=\"closed\"] {
        animation: zui-toast-out var(--zui-toast-move) var(--zui-motion-ease-in)
                   forwards;
    }"
    // Away from the corner on the way in and back towards it on the way out, so the movement agrees
    // with the edge the stack is anchored to.
    ":scope[data-corner=\"top-right\"] .zui-toast,
     :scope[data-corner=\"top-left\"] .zui-toast {
        animation: zui-toast-in-down var(--zui-toast-move) var(--zui-motion-ease-out);
    }"
    ":scope[data-corner=\"top-right\"] .zui-toast[data-state=\"closed\"],
     :scope[data-corner=\"top-left\"] .zui-toast[data-state=\"closed\"] {
        animation: zui-toast-out-up var(--zui-toast-move) var(--zui-motion-ease-in)
                   forwards;
    }"
    "@keyframes zui-toast-in {
        from { opacity: 0; transform: translateY(100%); }
        to { opacity: 1; transform: translateY(0); }
    }"
    "@keyframes zui-toast-out {
        from { opacity: 1; transform: translateY(0); }
        to { opacity: 0; transform: translateY(100%); }
    }"
    "@keyframes zui-toast-in-down {
        from { opacity: 0; transform: translateY(-100%); }
        to { opacity: 1; transform: translateY(0); }
    }"
    "@keyframes zui-toast-out-up {
        from { opacity: 1; transform: translateY(0); }
        to { opacity: 0; transform: translateY(-100%); }
    }"
    // The mark before the title. Pulled a little to the left of the text and given a little back on
    // the right, which is what makes the icon and the title read as one line rather than two.
    ".zui-toast__icon {
        display: flex;
        width: 16px;
        height: 16px;
        position: relative;
        align-items: center;
        justify-content: flex-start;
        flex: none;
        margin-left: -3px;
        margin-right: var(--zui-space-base);
    }"
    // The mark carries the kind on its own, because the surface is the same for every toast: the
    // colour of this one glyph is the whole of what tells a finished job from a failed one. Each
    // names the tone for that meaning, so an application that re-takes its tones re-takes these.
    ".zui-toast[data-kind=\"success\"] .zui-toast__icon { color: var(--zui-color-success); }"
    ".zui-toast[data-kind=\"info\"] .zui-toast__icon { color: var(--zui-color-info); }"
    ".zui-toast[data-kind=\"warning\"] .zui-toast__icon { color: var(--zui-color-warning); }"
    ".zui-toast[data-kind=\"error\"] .zui-toast__icon { color: var(--zui-color-destructive); }"
    ".zui-toast[data-kind=\"loading\"] .zui-toast__icon {
        animation: zui-toast-spin 1000ms var(--zui-motion-ease-linear) infinite;
    }"
    "@keyframes zui-toast-spin {
        from { transform: rotate(0deg); }
        to { transform: rotate(360deg); }
    }"
    ".zui-toast__text { display: flex; flex: 1 1 auto; flex-direction: column; gap: 2px; }"
    ".zui-toast__title {
        font-weight: var(--zui-type-weight-medium);
        line-height: 1.5;
        color: inherit;
    }"
    ".zui-toast__description {
        color: var(--zui-color-muted-foreground);
        font-weight: var(--zui-type-weight-normal);
        line-height: 1.4;
    }"
    // The two buttons. The action is the message's colours inverted, which is what makes it the one
    // thing on the toast that is unmistakably pressable; the cancel is a wash over the surface.
    ".zui-toast__action,
     .zui-toast__cancel {
        display: flex;
        align-items: center;
        flex: none;
        height: 24px;
        padding: 0 var(--zui-space-sm);
        margin-left: auto;
        border: none;
        border-radius: var(--zui-radius-sm);
        font-size: var(--zui-type-size-xs);
        font-weight: var(--zui-type-weight-medium);
        white-space: nowrap;
        transition: opacity var(--zui-toast-move) var(--zui-motion-ease-standard);
    }"
    ".zui-toast__action {
        background-color: var(--zui-color-popover-foreground);
        color: var(--zui-color-popover);
    }"
    ".zui-toast__cancel {
        background-color: color-mix(in oklab, var(--zui-color-foreground) 8%, transparent);
        color: var(--zui-color-popover-foreground);
    }"
    // A cancel and an action together: only the first of them takes the space before it.
    ".zui-toast__cancel + .zui-toast__action { margin-left: var(--zui-space-xs); }"
    ".zui-toast__action:focus-visible,
     .zui-toast__cancel:focus-visible {
        box-shadow: 0 0 0 2px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    // Out at the corner of the toast rather than inside it, which is where sonner puts it and what
    // keeps it clear of the title however long the title is.
    //
    // The pull past the corner is a length rather than a percentage of the disc, though the two
    // describe the same seven pixels: a percentage in a transform is resolved against the box the
    // matrix was interned with, and a disc whose matrix was interned before its own layout had
    // settled kept a translation measured against the wrong box — drawn adrift of its corner, and
    // hit-testable somewhere else again. The disc is twenty pixels by declaration, so the length
    // loses nothing.
    ".zui-toast__close {
        position: absolute;
        left: 0;
        top: 0;
        transform: translate(-7px, -7px);
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        padding: 0;
        border: 1px solid var(--zui-color-border);
        border-radius: 50%;
        background-color: var(--zui-color-popover);
        color: var(--zui-color-popover-foreground);
        z-index: 1;
        transition: background-color var(--zui-motion-duration-slow) var(--zui-motion-ease-standard),
                    border-color var(--zui-motion-duration-slow) var(--zui-motion-ease-standard);
    }"
    ".zui-toast__close:hover {
        background-color: var(--zui-color-muted);
        border-color: var(--zui-color-input);
    }"
    ".zui-toast__close:focus-visible {
        box-shadow: 0 0 0 2px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
}
