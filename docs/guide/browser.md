# Building a browser on zgui

zgui ships no HTML crate and no JavaScript engine, and the core carries no web coupling at all. That
is not an omission — it is the requirement that shaped the seams. Everything a document *language*
brings, as opposed to a document *core*, arrives through a trait a consumer implements.

This guide names those extension points in the order somebody putting a document language on top
would meet them. It applies just as well to a diagram format, a report renderer, or an application
with an embedded scripting language; a browser is simply the case that needs all of them.

## What you get for free

Before naming what you would have to write: the cascade, selector matching, inheritance, custom
properties, media and container queries, computed values, flexbox, grid, block and float layout,
inline layout and bidirectional text, shaping and line breaking, fragment generation, hit testing,
stacking contexts, damage tracking, the display list, GPU compositing, path rendering, the
accessibility tree, input dispatch with capture and bubble, scrolling with chaining and momentum,
transitions and animations, and text editing with a caret, selection, undo and composition.

None of that has a web concept in it, and none of it needs replacing.

## The extension points

### 1. A vocabulary of element names — `Tag` (L6)

Element names are types. A marker type implementing `zgui_elements::Tag` says what an element is
called; the name is what selectors match and what the user-agent sheet gives layout defaults to.

```rust,ignore
pub struct Div;
impl Tag for Div {
    fn name() -> ElementName { ElementName::new("div") }
}
```

Implementing `Tag` outside the element crate is how a document language of one's own joins the same
builder machinery. `html::div()` in a view is a call to a function that returns an `Element` over
that language's marker. Typed attributes come with the element, so an editor completes attribute
names and reports a mistake against the item that defines them.

You would also supply a user-agent stylesheet for your vocabulary, installed at
`SheetOrigin::UserAgent`, which is where a document language's display defaults belong.

### 2. What a document language derives from markup — `PresentationalHints` (L3)

HTML gives `width`, `height`, `bgcolor` and `align` a meaning the cascade has to honour, at a level
of its own below every author rule and above the user-agent origin. Nothing in the document core
knows those attributes exist, so the declarations they stand for are contributed from outside, once
per restyled element.

```rust,ignore
impl PresentationalHints for HtmlHints {
    fn hints_for(
        &self,
        element: Node<'_>,
        visited: VisitedHandlingMode,
        out: &mut dyn Push<ApplicableDeclarationBlock>,
    ) { /* … */ }
}
```

Blocks pushed here are the *presentational hint* origin, so an author rule of any specificity
overrides them and an `!important` author rule cannot be overridden by them. Push order is cascade
order within the origin: later blocks win. `visited` says which half of a link's style is being
computed, so a `link`/`vlink`/`alink` attribute can answer differently for the two.

The default implementation contributes nothing, and that is not a placeholder: a document with no
markup language on top of it has no legacy attributes.

### 3. What counts as a link — `LinkResolver` (L3)

`:link`, `:visited` and `:any-link` are the only selectors in CSS whose answer depends on a concept
no document core can define for itself: what counts as a link is a property of the document
language, and whether one has been visited is a property of a browsing history.

```rust,ignore
impl LinkResolver for MyLinks {
    fn is_link(&self, element: Node<'_>) -> bool { /* … */ }
    fn is_visited(&self, element: Node<'_>) -> bool { /* … */ }
}
```

The resolver is **not** consulted during selector matching. It is consulted when a node's attributes
change, and its answer is folded into that node's interaction state — the same word every other
state pseudo-class is answered from. That is a correctness requirement, not an optimisation: the
style engine invalidates `:link` and `:visited` by comparing state words across a mutation, so an
answer that lived only inside the matcher would change without invalidating anything, and the old
style would stay on the screen.

The consequence for an implementor is that the answer must be a pure function of what the document
holds. A page finishing loading or a history entry appearing takes effect when the affected nodes are
refreshed, not spontaneously.

The default answer for `is_visited` is "no", and a consumer with no browsing history should keep it:
a document that reports every link unvisited leaks nothing about where its user has been.

### 4. Where a stylesheet's source comes from — `SheetLoader` (L3)

A document core that could fetch a URL would need a network stack, a cache, a security policy and an
idea of what a URL is. A consumer that wants `@import` already has all four.

```rust,ignore
impl SheetLoader for MyLoader {
    fn load(&self, base: &str, href: &str) -> SheetRequest {
        // SheetRequest::Ready(text) | ::Pending | ::Rejected
    }
}
```

`base` and `href` are text rather than a parsed URL type, because resolving one against the other is
the loader's decision: only a consumer with a document base, a cache and a policy can say what
`../theme.css` means, and a core that took a position on it would have to be overridden rather than
implemented.

`Ready` continues the parse in the same call, which is what `@import` needs — imported rules take
the position of the `@import` in the importing sheet, and a rule set cannot be spliced into the
middle of a sheet afterwards. `Pending` contributes nothing for now, and delivering the text later
takes effect by *replacing the sheet*, not by patching one already parsed. With no loader installed
every request is refused, so `@import` is reported as a parse error rather than silently doing
nothing.

`load` is called on the thread that is parsing and never from a style worker.

### 5. How big outside content is — `ReplacedContent` (L3) and `MeasureContent` (L4)

An image, a video, a canvas, an embedded document: layout has to know its natural size, ratio and
baseline, and cannot work any of them out.

```rust,ignore
impl ReplacedContent for MyMedia {
    fn intrinsic(&self, id: ReplacedId) -> Intrinsic { /* … */ }
}
```

Two things about this seam are deliberate. It does **not** paint — a replaced node's pixels are an
atlas sprite or an external texture, which belong to the scene rather than to the document, and a
document that could name them would have an edge to the renderer it is designed not to have. The
painting half is a separate hook installed beside the scene, keyed by the same `ReplacedId`. And it
must be `Send + Sync`, because it is consulted from layout workers, while a paint source holds
device resources that usually cannot be.

An identifier naming a node that is gone must be answered with a default rather than by panicking: a
frame in flight can outlive the node it was measuring.

`MeasureContent` is the wider seam beneath it — measuring replaced content, shaping a paragraph,
breaking one into lines, computing a strut, claiming a brush slot. Most consumers get this for free
by installing a text engine; a consumer bringing its own text stack implements it.

### 6. Frame hooks for a script engine — `HostBinding` (L7)

The document carries no scripting language, and the hooks a scripting language needs are frame-loop
concepts rather than document ones: a callback that runs before the next paint, a checkpoint at
which queued work is drained, and a chance to see an event before the ordinary listeners do. All
three are questions about *when*, and only the loop knows when.

```rust,ignore
pub trait HostBinding {
    fn before_dispatch(&mut self, target: Option<NodeId>, event: EventKind) -> bool { true }
    fn checkpoint(&mut self) {}
    fn before_paint(&mut self, timestamp: Timestamp) {}
    fn shutting_down(&mut self) {}
}
```

Every method has a do-nothing default, so an implementation states only the hooks it wants. The
order within a frame is fixed and is the whole content of the contract:

1. `before_dispatch`, once per event, before the first listener on its path. Answering `false` means
   the event is not dispatched at all, which is what an engine intercepting an event for its own
   dispatcher does.
2. `checkpoint`, after the frame's reactive work has settled. Drain the engine's queued work here.
   Direct document changes are included in the same frame's restyle. A signal write needs a later
   flush.
3. `before_paint`, after layout and geometry observation delivery, and before paint emission. Use
   this hook for work that must occur at that boundary. A document or signal change made here is
   processed in the next frame because restyle and layout are complete. The frame timestamp lets
   the callback use the same clock value as the rest of the frame.

Nothing in this framework implements `HostBinding`. It exists so that somebody building a browser
has one object to install rather than a fork of the loop.

### 7. Fonts — `FontSource`, `FontMetricsSource`, `ParagraphShaper`, `GlyphRaster` (L4)

Four seams, so a font engine can be replaced or absent without a consumer changing:

| Seam | What plugs in | Who asks |
|---|---|---|
| `FontMetricsSource` | a font system, or `FixedMetrics` | the cascade, resolving `ex`, `ch`, `cap`, `ic` |
| `FontSource` | a font collection | face resolution and `@font-face` |
| `ParagraphShaper` | a text engine | layout, once per paragraph and many times per width |
| `GlyphRaster` | a rasteriser | painting, once per distinct glyph |

Preserve the split between shaping and breaking. Shaping is the expensive operation. Line breaking
is the cheap operation. Layout can ask for the size of one paragraph at many candidate widths while
it resolves a flex or grid container. Do not shape the paragraph again for each width.

### 8. A different node tree entirely — `Dom`, `ViewHost`, `EventSink` (L6)

The preceding seams put a language *on* zgui's document. These three seams put zgui's *view layer*
on something else, such as a browser's own nodes, a transcript recorder, or a remote tree.

| Trait | What it answers |
|---|---|
| `Dom` | create, insert, detach, set an attribute, register a listener, observe geometry |
| `ViewHost` | where a box ended up, what is focused, what is selected, what to run half a second from now |
| `EventSink` | commands a handler issues, carried out after the dispatch that issued them |

`Dom` is deliberately small and deliberately imperative — create a node, put it somewhere, change one
thing about it — because that is the set a retained view layer issues: roughly ten calls per
*changed* node per frame. A new backend is an afternoon rather than a project.

What an implementation promises:

- Every `NodeId` it returns carries its own `DocumentId`, and a handle from another document is a
  programming error it may assert on in debug builds.
- `insert` **moves** a node already in the tree rather than duplicating it.
- `detach` removes a node from its parent and keeps it usable, because a view detaches and
  reattaches subtrees as its content changes.
- A setter with `None` removes the thing rather than setting it to an empty value.

`ViewHost` is separate from `Dom` so that both stay small and each is implementable on its own: a
backend can bring its node tree up first and answer geometry with nothing until it has a layout
engine. Every geometry answer is *as of the last completed frame* — reading layout mid-build cannot
be made both correct and cheap, and a framework that pretends otherwise is one that thrashes layout.
A view that needs to react to geometry as it changes registers an observation instead.

`EventSink` exists because a handler runs while the document is mid-mutation, so a command that took
effect immediately would re-enter a mutation that has not finished. Everything a handler asks for is
appended and carried out once the dispatch completes.

`zgui-view-dom` is the implementation of these over zgui's own document, and is worth reading as the
worked example.

### 9. A renderer or a platform of your own (L1/L2)

Below all of this, `Renderer`, `VectorRaster` and the `zgui-platform` traits are replaceable in the
same way. See [writing a `Renderer`](renderer.md).

## What a browser would still have to bring

Being honest about the size of the remaining job: a network stack, a URL type and a security policy;
an HTML parser and its error recovery; the DOM APIs a script engine expects, over `Dom`; a
JavaScript engine, attached through `HostBinding`; forms, history, navigation and sessions; and the
web platform's own APIs. zgui gives you a styling, layout, text, paint, render, input and
accessibility engine with no web coupling. It does not give you the web platform, and it does not
pretend the remainder is small.

## What is deliberately not extensible

The cascade's own rules, the invalidation lattice, the damage model, the display-list format and the
frame order are not seams and will not become them. Each is a single owner of a decision, and a
second owner of any of them is how two parts of a pipeline come to disagree about what changed —
which shows up as wrong pixels rather than as an error.
