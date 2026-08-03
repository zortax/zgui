# The styling model

Appearance and layout in zgui are CSS: real selectors, the real cascade, real inheritance, custom
properties, media and container queries, flexbox, grid, gradients, filters and transforms. There is
no styling DSL to learn and no subset dialect. What differs from a browser is what the document
underneath is, and what the framework does with a change once it has one.

## Where a sheet comes from

Three origins, in the cascade's own order. For declarations of equal specificity a later origin
wins; for `!important` declarations the order reverses, so a user-agent `!important` rule beats an
author one.

| Origin | What is in it |
|---|---|
| `UserAgent` | This framework's own sheet: the element vocabulary's display defaults, box sizing, the focus ring, selection colours, scrollbar metrics. |
| `User` | Overrides supplied by whoever runs the application, including end-user themes. |
| `Author` | The component library's sheets, then the application's, in registration order. |

An application installs an author sheet with `App::with_stylesheet`. A sheet's source is either its
text or a name the document's installed loader resolves — see [building a browser on
zgui](browser.md) for the loader seam, which is also what makes `@import` work.

## The element vocabulary already has a layout

Element names are not divs waiting for a rule. `row` is a horizontal flex container, `column` a
vertical one, `text` is inline, `control` is a focusable box. That is the user-agent sheet, and it
means a view that has had no CSS written for it still lays out sensibly.

```rust,ignore
view! {
    column(class = "card") {
        text(class = "card__title") {"Total"}
        row(class = "card__row") {
            text {"1,204"}
        }
    }
}
```

## Three ways to write CSS

**A sheet, checked at compile time.** `css!` parses the block where it is written, so an
unterminated string, an unbalanced block or a declaration with no value is a compile error pointing
at the source rather than a warning logged once when the sheet loads.

```rust,ignore
const SHEET: &str = css!(
    ".card { padding: 1rem; border-radius: 8px; }
     .card__title { font-weight: 600; }"
);
```

**A component's own sheet, scoped.** `style!` generates a type with `CLASS` and `CSS` constants.
`CLASS` is unique to the name and sheet text. The macro rewrites `:scope` to that class at compile
time, so the rules need no run-time rewriting. The component must install `CSS` and put `CLASS` on
the element that owns the scope. Installing the same name and text again is a no-op. Installing new
text under the same name replaces the sheet without moving it in cascade order.

```rust,ignore
style! { pub Button =>
    ":scope { display: inline-flex; align-items: center; }"
    ":scope[data-disabled] { opacity: .5; }"
}

#[component]
fn StyledButton(children: Children) -> impl IntoView {
    install_stylesheet(Button::CLASS, Button::CSS);
    view! {
        control(class = Button::CLASS) {{children.into_view_once()}}
    }
}
```

**A variant table.** `variants!` turns a table of visual axes into one enumeration per axis, a
`class_list()` that concatenates them in a *stable* order — so a class list is diffable and a
transcript is deterministic — and a `data_attributes()` that reports the same choice as `data-*`
attributes. Match on the data attributes in the sheet rather than concatenating class strings at run
time.

```rust,ignore
variants! {
    pub ButtonVariants {
        base: "zui-btn",
        variant: { Default => "zui-btn--default", Outline => "zui-btn--outline" } = Default,
        size: { Sm => "zui-btn--sm", Md => "" } = Md,
    }
}
```

## Styling from a view

| Written | Means |
|---|---|
| `class="a b"` | the whole class list |
| `class:name=on` | one class, toggled — the value may be a signal or a closure |
| `style="…"` | the whole inline style text |
| `style:property=value` | one declaration |
| `var:--name=value` | one custom property |
| `attr:name=value` | an arbitrary attribute, which selectors can see |
| `state:name=on` | one of the interaction states a view may assert |
| `custom_state:name=on` | an author-defined state, matched by `:state(name)` |

Whether one of these is static or dynamic is decided by its **type**, not by an annotation. A
literal is written once at build time; a signal or a closure gets exactly one effect that writes
only when the value actually changes.

Prefer `class:` and `custom_state:` over `style:`. A class toggle is a class-list write the
invalidation machinery can filter cheaply; an inline declaration is a new declaration block to
cascade.

## What a style change costs

This is the part worth understanding, because it is what makes a large interface stay cheap.

**A mutation is filtered before the engine sees it.** The compiled rule set can answer three
questions about a change without restyling anything: does any selector mention this class name; does
any selector mention this attribute name; and which interaction-state bits could any selector
matching this element possibly depend on. Toggling a class no sheet mentions, or writing a state bit
outside that element's mask, needs no snapshot and no restyle at all.

**A restyle visits what owes one.** Invalidation is recorded on nodes as obligations, and the
traversal descends only where obligations live. There is no per-frame walk of the document.

**A computed style is a shared pointer.** Two elements that cascaded to the same result share one
allocation, and so do the individual property groups behind it. Everything downstream can key a
cache on the pointer, which is how a thousand elements lower to one lowering.

**What changed decides what is owed below.** The engine compares the old computed style with the new
one and turns the difference into obligations: a colour change is repaint damage; a width change is
relayout; a font change is a re-shape. Nothing below the cascade has to guess.

The practical consequences for someone writing an application:

- Changing a custom property on a container is cheap and inherits — it is the right way to theme.
- A media-query boundary that a resize does not cross disturbs no origin, and the resize does no
  restyle work at all.
- Replacing a whole sheet is the expensive operation, because for that one frame the filters above
  stop being answers and everything is conservatively assumed to matter. Prefer toggling a class or
  writing a custom property to swapping sheets.

## Custom properties and theming

Custom properties inherit, cascade and are read with `var()`, exactly as in CSS. The component
library's tokens are built on them, which is why a theme change is a handful of declarations on the
root rather than a rebuild of anything.

```css
:root { --surface: #14161a; --on-surface: #f2f4f8; }
:root[data-theme="light"] { --surface: #fbfbfd; --on-surface: #14161a; }
.card { background: var(--surface); color: var(--on-surface); }
```

Colour scheme is available to sheets as a media feature as well, so a component can respond to the
desktop's preference without an application wiring anything.

## Where the boundary is

Parity here means parity with **what the style engine and the vector stack actually support**. That
boundary is measured rather than claimed: `docs/parity.md` is generated by the conformance harness,
counts *implemented*, *not yet implemented* and *out of reach* as three separate numbers, and every
out-of-reach row says what an application should write instead.

A property counts as implemented only when setting it on a fixture visibly changes the fragment tree
or the answer hit testing gives. If the deterministic harness cannot observe the consequence, the
property must be listed with a reason. An unlisted declaration with no observable consequence fails
the build.

If something does not work, that file is the first place to look; it will either say what to write
instead or it is a bug.

## Debugging a style

The inspector (`zgui-devtools`, F12) shows, for the picked element: its selector, its fragment
count, a nested border/padding/content diagram, the twenty-four layout longhands, and every property
that is not at its initial value. That last list is usually the fastest answer to "why is this
element the size it is".
