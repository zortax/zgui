//! What a text field looks like, in tokens.
//!
//! The caret is not here. It is drawn by the framework, from the editing model that owns where it
//! is, over the lines the frame actually laid out — which is the only place that answer exists. A
//! sheet that drew one as well would put a second caret on every focused field, in a place only the
//! sheet believes in.
//!
//! # The ring
//!
//! A focused field is marked twice: its border takes the ring colour, and a three-pixel band of the
//! same colour at half strength is drawn just outside it. That band is a shadow rather than an
//! outline, so that it stacks with the field's own lift instead of replacing it and so that it can
//! be moved through — an outline arrives at full strength on the frame focus lands.

use zgui::style;

style! { pub InputStyle =>
// No fill of its own on a light page: the field is a hole in whatever it sits on, and its
// border is what makes it a field. The dark scheme fills it faintly instead, because a hairline
// border alone disappears against a dark surface.
":scope {
        position: relative;
        display: flex;
        flex-direction: row;
        align-items: center;
        overflow: hidden;
        height: 36px;
        width: 100%;
        min-width: 0;
        padding: 4px 12px;
        border: 1px solid var(--zui-color-input);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        white-space: pre;
        transition-property: color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
// Wrong shows without focus — the border alone — and gains a ring of its own once focus
// arrives, in the destructive colour rather than the neutral one. A wrong field wearing the
// neutral ring would be saying two things at once.
":scope:invalid { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus-visible {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
// `:empty` is *this field holds no text*, answered by the document itself: the element's only
// children are the text nodes the editing model writes. Out of flow, so that it neither moves
// the text nor pushes the insertion point along in front of it.
":scope:empty::before {
        content: var(--zui-field-placeholder, \"\");
        position: absolute;
        top: 0;
        bottom: 0;
        left: 12px;
        right: 12px;
        display: flex;
        flex-direction: row;
        align-items: center;
        color: var(--zui-color-muted-foreground);
    }"
}

style! { pub TextareaStyle =>
":scope {
        position: relative;
        display: block;
        overflow: hidden;
        width: 100%;
        min-height: 64px;
        padding: 8px 12px;
        border: 1px solid var(--zui-color-input);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-control-field);
        color: var(--zui-color-foreground);
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        box-shadow: var(--zui-shadow-xs);
        outline: none;
        white-space: pre-wrap;
        transition-property: color, box-shadow, border-color;
        transition-duration: var(--zui-motion-duration-normal);
        transition-timing-function: var(--zui-motion-ease-standard);
    }"
":scope:focus-visible {
        border-color: var(--zui-color-ring);
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent),
                    var(--zui-shadow-xs);
    }"
":scope:invalid { border-color: var(--zui-color-destructive); }"
":scope:invalid:focus-visible {
        border-color: var(--zui-color-destructive);
        box-shadow: 0 0 0 3px var(--zui-color-control-ring-invalid),
                    var(--zui-shadow-xs);
    }"
":scope:disabled { opacity: 0.5; pointer-events: none; }"
":scope:empty::before {
        content: var(--zui-field-placeholder, \"\");
        position: absolute;
        top: 8px;
        left: 12px;
        right: 12px;
        color: var(--zui-color-muted-foreground);
    }"
}
