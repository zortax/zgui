//! What an aspect ratio box looks like, in tokens.

use zgui::style;

style! { pub AspectRatioStyle =>
    // The ratio is a custom property rather than a rule per value: a caller's ratio is a number
    // known at run time, and a class per number is a sheet that grows with the application.
    ":scope {
        position: relative;
        display: block;
        width: 100%;
        aspect-ratio: var(--zui-aspect-ratio, 1);
    }"
    ":scope > * { width: 100%; height: 100%; }"
}
