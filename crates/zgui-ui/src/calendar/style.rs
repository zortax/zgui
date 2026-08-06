//! What a calendar looks like, in tokens.

use zgui::style;

style! { pub CalendarStyle =>
    // One number decides the whole grid: a day is a square of it, the weekday names sit over
    // columns of it, and the two month buttons are exactly one of it so the caption row is the
    // height of a day.
    ":scope {
        --zui-calendar-cell: 32px;
        display: flex;
        flex-direction: column;
        padding: var(--zui-space-md);
        background-color: var(--zui-color-background);
        color: var(--zui-color-foreground);
        font-size: var(--zui-type-size-sm);
        line-height: var(--zui-type-leading-sm);
        width: fit-content;
    }"
    // Months sit side by side and the two buttons span all of them, so a two-month calendar steps
    // both at once from the far ends of the whole block rather than growing a second pair.
    ":scope .zui-calendar__months {
        position: relative;
        display: flex;
        flex-direction: row;
        gap: var(--zui-space-lg);
    }"
    ":scope .zui-calendar__month {
        display: flex;
        flex-direction: column;
        gap: var(--zui-space-lg);
        width: 100%;
    }"
    ":scope .zui-calendar__nav {
        position: absolute;
        left: 0;
        right: 0;
        top: 0;
        display: flex;
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
        gap: var(--zui-space-base);
    }"
    // The caption is kept clear of the buttons by a cell's width on each side rather than by having
    // the buttons in the same row: the nav is over the whole block and the caption is per month.
    ":scope .zui-calendar__caption {
        display: flex;
        align-items: center;
        justify-content: center;
        height: var(--zui-calendar-cell);
        width: 100%;
        padding: 0 var(--zui-calendar-cell);
        box-sizing: border-box;
    }"
    ":scope .zui-calendar__heading {
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-medium);
    }"
    ":scope .zui-calendar__step {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: var(--zui-calendar-cell);
        height: var(--zui-calendar-cell);
        flex: none;
        padding: 0;
        border: none;
        border-radius: var(--zui-radius-md);
        background-color: transparent;
        color: inherit;
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-calendar__step:hover {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope .zui-calendar__step:focus-visible {
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope .zui-calendar__step[data-disabled=\"true\"] { opacity: 0.5; }"
    // The grid's tracks are the seven days, and there is no gap between the columns: a range drawn
    // across a gapped grid is a row of separate blocks rather than one band.
    ":scope .zui-calendar__grid {
        display: grid;
        grid-template-columns: repeat(7, minmax(var(--zui-calendar-cell), 1fr));
        row-gap: var(--zui-space-sm);
        width: 100%;
    }"
    ":scope .zui-calendar__week { display: contents; }"
    ":scope .zui-calendar__weekday {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 20px;
        border-radius: var(--zui-radius-md);
        color: var(--zui-color-muted-foreground);
        font-size: 12.8px;
        font-weight: var(--zui-type-weight-normal);
    }"
    // A day is two elements: the cell, which carries the band a chosen span is drawn as, and the
    // mark inside it, which carries the pill. One element cannot be both a rounded pill and the
    // square continuation of the band it sits in.
    ":scope .zui-calendar__day {
        position: relative;
        display: flex;
        aspect-ratio: 1 / 1;
        width: 100%;
        min-height: var(--zui-calendar-cell);
        padding: 0;
        border: none;
        background-color: transparent;
        color: inherit;
        text-align: center;
    }"
    ":scope .zui-calendar__day-mark {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        justify-content: center;
        min-width: var(--zui-calendar-cell);
        border-radius: var(--zui-radius-md);
        font-size: var(--zui-type-size-sm);
        font-weight: var(--zui-type-weight-normal);
        line-height: var(--zui-type-leading-sm);
        transition: background-color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard),
                    color var(--zui-motion-duration-normal) var(--zui-motion-ease-standard);
    }"
    ":scope .zui-calendar__day:hover .zui-calendar__day-mark {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    // Today is the cell's own paint, so a day that is both today and chosen shows the pill over it
    // rather than instead of it.
    ":scope .zui-calendar__day[data-today=\"true\"] {
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
    }"
    ":scope .zui-calendar__day[data-today=\"true\"][data-selected=\"true\"] { border-radius: 0; }"
    ":scope .zui-calendar__day[data-outside=\"true\"] { color: var(--zui-color-muted-foreground); }"
    ":scope .zui-calendar__day[data-selected-single=\"true\"] .zui-calendar__day-mark,
      :scope .zui-calendar__day[data-range=\"start\"] .zui-calendar__day-mark,
      :scope .zui-calendar__day[data-range=\"end\"] .zui-calendar__day-mark {
        background-color: var(--zui-color-primary);
        color: var(--zui-color-primary-foreground);
        border-radius: var(--zui-radius-md);
    }"
    // The band. The two ends paint the cell as well as the mark, so the square corners behind a
    // rounded pill are filled and the span reads as one shape rather than three.
    ":scope .zui-calendar__day[data-range=\"start\"] {
        background-color: var(--zui-color-accent);
        border-radius: var(--zui-radius-md) 0 0 var(--zui-radius-md);
    }"
    ":scope .zui-calendar__day[data-range=\"end\"] {
        background-color: var(--zui-color-accent);
        border-radius: 0 var(--zui-radius-md) var(--zui-radius-md) 0;
    }"
    ":scope .zui-calendar__day[data-range=\"middle\"] { border-radius: 0; }"
    ":scope .zui-calendar__day[data-range=\"middle\"] .zui-calendar__day-mark {
        background-color: var(--zui-color-accent);
        color: var(--zui-color-accent-foreground);
        border-radius: 0;
    }"
    // Lifted while focused, so the ring is drawn over the days on either side instead of under them.
    ":scope .zui-calendar__day:focus-visible { z-index: 10; }"
    ":scope .zui-calendar__day:focus-visible .zui-calendar__day-mark {
        box-shadow: 0 0 0 3px color-mix(in oklab, var(--zui-color-ring) 50%, transparent);
    }"
    ":scope .zui-calendar__day:disabled {
        color: var(--zui-color-muted-foreground);
        opacity: 0.5;
        pointer-events: none;
    }"
}
