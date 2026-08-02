//! What a progress bar looks like, in tokens.

use zgui::style;

style! { pub ProgressStyle =>
    ":scope {
        position: relative;
        overflow: hidden;
        height: 8px;
        width: 100%;
        border-radius: var(--zui-radius-full);
        background-color: color-mix(in oklab, var(--zui-color-primary) 20%, transparent);
    }"
    // The filled part is positioned from the left and sized by a custom property the component
    // writes, so a change of value is one declaration rather than a rebuilt subtree.
    //
    // The resting `translateX(0)` is for the indeterminate slide below: an animation whose
    // keyframes would bring a transform into existence — rather than move one the box already has
    // — is sent back through the cascade instead of being sampled, because whether a box is
    // transformed decides its stacking context and what it contains. Declaring the identity here
    // keeps the slide on the sampled path.
    ":scope > .zui-progress__fill {
        height: 100%;
        width: var(--zui-progress-fraction, 0%);
        border-radius: inherit;
        background-color: var(--zui-color-primary);
        transform: translateX(0);
        transition: width var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope[data-state=\"indeterminate\"] > .zui-progress__fill {
        width: 40%;
        animation: zui-progress-slide 1200ms var(--zui-motion-ease-standard) infinite;
    }"
    "@keyframes zui-progress-slide {
        0% { transform: translateX(-100%); }
        100% { transform: translateX(250%); }
    }"
}
