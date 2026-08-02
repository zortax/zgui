//! What a spinner looks like, in tokens.

use zgui::style;

style! { pub SpinnerStyle =>
    // The turn is linear because it repeats: an eased rotation visibly hesitates every time one
    // revolution meets the next, and a spinner that hesitates reads as a spinner that has stopped.
    //
    // The resting `rotate(0deg)` is load-bearing. Whether a box is transformed at all decides its
    // stacking context and what it contains, so those answers live in the shared style — and an
    // animation whose keyframes would *bring a transform into existence* is sent back through the
    // cascade instead of being sampled. With the identity rotation declared, the turn only moves a
    // transform the box already has.
    ":scope {
        display: inline-block;
        width: 16px;
        height: 16px;
        flex-shrink: 0;
        border: 2px solid currentColor;
        border-top-color: transparent;
        border-radius: var(--zui-radius-full);
        transform: rotate(0deg);
        animation: zui-spinner-turn 1000ms var(--zui-motion-ease-linear) infinite;
    }"
    "@keyframes zui-spinner-turn {
        0% { transform: rotate(0deg); }
        100% { transform: rotate(360deg); }
    }"
}
