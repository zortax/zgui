//! What a skeleton looks like, in tokens.

use zgui::style;

style! { pub SkeletonStyle =>
    // The pulse moves the box's *opacity*, which is the reference's own pulse: `animate-pulse` is
    // an opacity fade to a half and back over two seconds, on this exact curve, over `bg-accent`.
    //
    // It used to move the fill's colour between two named ramp steps instead, and the far step is
    // why that could not stay: `--zui-scale-neutral-5` is one fixed grey whatever the scheme, so
    // on a dark page the skeleton pulsed *up* to a mid grey far brighter than any surface around
    // it. The accent token is the one colour that is right in both schemes, and fading the whole
    // box keeps the swing proportional to it — on a card or on the page, in either theme.
    ":scope {
        background-color: var(--zui-color-accent);
        border-radius: var(--zui-radius-md);
        animation: zui-skeleton-pulse 2000ms cubic-bezier(0.4, 0, 0.6, 1) infinite;
    }"
    "@keyframes zui-skeleton-pulse {
        0% { opacity: 1; }
        50% { opacity: 0.5; }
        100% { opacity: 1; }
    }"
}
