//! What a switch looks like, in tokens.
//!
//! The track carries a transparent border rather than padding. Both inset the thumb by a pixel on
//! every side, but only the border keeps doing it while the focus ring is drawn: the ring is a
//! shadow around the border box, so a track that reached the thumb through padding would put the
//! ring a pixel inside where the eye reads the edge.

use zgui::style;

style! { pub SwitchStyle =>
":scope {
        display: inline-flex;
        align-items: center;
        width: 32px;
        height: 18.4px;
        flex: none;
        border: 1px solid transparent;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-control-switch-off);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        transition-property: background-color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope[data-size=\"sm\"] { width: 24px; height: 14px; }"
":scope:checked { background-color: var(--zui-color-primary); }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
// The thumb rests at `translateX(0)`, written out rather than left implicit. Whether a box is
// transformed at all decides its stacking context and what it contains, so those answers live in
// the shared style and an animation that would *bring a transform into existence* is sent back
// through the cascade instead of being sampled. With the identity transform declared, sliding to
// the checked position only moves a transform the thumb already has.
":scope > .zui-switch__thumb {
        width: 16px;
        height: 16px;
        flex: none;
        border-radius: var(--zui-radius-full);
        background-color: var(--zui-color-control-switch-thumb);
        pointer-events: none;
        transform: translateX(0);
        transition-property: transform;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope[data-size=\"sm\"] > .zui-switch__thumb { width: 12px; height: 12px; }"
":scope:checked > .zui-switch__thumb {
        background-color: var(--zui-color-control-switch-thumb-on);
    }"
// Its own width less the two pixels of border it travels between, which lands the thumb flush
// against the far end whatever size the switch is.
":scope:checked > .zui-switch__thumb { transform: translateX(14px); }"
":scope[data-size=\"sm\"]:checked > .zui-switch__thumb { transform: translateX(10px); }"
// On a dark page the thumb is the page's own text colour when the switch is off and the tint's
// text colour when it is on, so the knob keeps its contrast against a track that has just gone
// from grey to near-white.
}
