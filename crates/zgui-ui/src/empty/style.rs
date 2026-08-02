//! What an empty state looks like, in tokens.

use zgui::style;

style! { pub EmptyStyle =>
    ":scope {
        display: flex;
        flex: 1;
        min-width: 0;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--zui-space-xl);
        padding: var(--zui-space-xl);
        border-style: dashed;
        border-radius: var(--zui-radius-lg);
        text-align: center;
    }"
    // On a wide surface the panel breathes: the same heading in the middle of a full-width table
    // needs twice the room round it before it stops reading as a notice bar.
    "@media (min-width: 768px) { :scope { padding: 48px; } }"
    ".zui-empty__header {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--zui-space-sm);
        max-width: 384px;
        text-align: center;
    }"
    ".zui-empty__media {
        display: flex;
        flex-shrink: 0;
        align-items: center;
        justify-content: center;
        margin-bottom: var(--zui-space-sm);
        background-color: transparent;
    }"
    ".zui-empty__media .zui-icon { pointer-events: none; flex: none; }"
    ".zui-empty__media[data-variant=\"icon\"] {
        width: 40px;
        height: 40px;
        border-radius: var(--zui-radius-lg);
        background-color: var(--zui-color-muted);
        color: var(--zui-color-foreground);
        --zui-icon-md: 24px;
    }"
    ".zui-empty__title {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-lg);
        font-weight: var(--zui-type-weight-medium);
        line-height: var(--zui-type-leading-lg);
        letter-spacing: var(--zui-type-tracking-tight);
    }"
    ".zui-empty__description {
        font-family: var(--zui-type-family-sans);
        font-size: var(--zui-type-size-sm);
        line-height: 1.625;
        color: var(--zui-color-muted-foreground);
    }"
    ".zui-empty__content {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: var(--zui-space-lg);
        width: 100%;
        min-width: 0;
        max-width: 384px;
        font-size: var(--zui-type-size-sm);
    }"
}
