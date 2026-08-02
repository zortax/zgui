# The reactive model, and its three `Send` escapes

State in zgui is signals. A component function runs **once**; the closures inside it run whenever
what they read changes. Nothing is diffed, nothing is re-rendered, and there is no virtual tree.

This guide covers the model, the three rules that make it safe, and — because it is the thing that
most often stops a build with a confusing error — the three places where the underlying engine's
`Send + Sync` requirement has an explicit escape.

## The model in one example

```rust,ignore
#[component]
fn Counter() -> impl IntoView {
    let count = RwSignal::new(0);

    view! {
        column {
            text {{move || count.get().to_string()}}
            control(on:click = move |_| count.update(|n| *n += 1)) {"+"}
        }
    }
}
```

`Counter` runs once. `move || count.get().to_string()` is a **reactive hole**: one effect, which
writes the text node only when the string actually changes. The same expression without the closure
— `count.get().to_string()` — is written once and never again. The difference is the type, not a
keyword, and that is the whole static-versus-dynamic story.

## The three rules

### One thread runs reactivity

`install()` claims the calling thread as the UI thread and installs a task pool that lives on it.
Signals may be **read and written** from any thread; tasks only ever **run** on the UI thread.
`assert_ui_thread` is the debug guard.

### Nothing exists outside an owner

Contexts, stored values and every arena-backed handle are attached to the `Owner` that was current
when they were created. With no current owner they are silently discarded and permanently leaked —
no panic, no log, in release builds.

`Mounted` is the protocol that keeps one owner per mounted node and disposes of it *synchronously*
on unmount, so a component that goes away takes its effects, its stored values and its cleanups with
it in the same frame. `assert_owner` is the debug guard, and the debug-build assertions in
`provide_context` and friends exist precisely because the failure is otherwise invisible.

### Work only happens at the flush

Writing a signal marks its observers and wakes their tasks. It does not run them.

The frame loop calls `flush()` once per frame, which polls every ready task to a stall under a
bounded iteration budget and reports whether another frame is owed. This is why a listener can write
five signals without five restyles: they are all marked, and one flush settles them.

A wake that arrives from **anywhere else** — a worker thread, a completed download, a timer — is
routed to the `FrameWaker`, which asks the platform for a redraw. Without that second edge, a task
made ready by a worker thread would be marked ready and then wait for the user to move the mouse.
An implementation of `FrameWaker` must be callable from any thread, must not block, and must be
idempotent: a hundred wakes between two frames must cost one frame, not a hundred.

```rust,ignore
let (count, doubled) = root.with(|| {
    let count = RwSignal::new(1);
    let doubled = RwSignal::new(0);
    let effect = RenderEffect::new(move |_| doubled.set(count.get() * 2));
    (count, doubled, effect)
});

assert_eq!(doubled.get(), 2);   // a render effect's first run is synchronous
count.set(21);
assert_eq!(doubled.get(), 2);   // every later run waits for the flush
flush();
assert_eq!(doubled.get(), 42);
```

## What to reach for

| Want | Use |
|---|---|
| Mutable state | `RwSignal<T>`, or `signal()` for a separate reader and writer |
| A derived value, cached | `Memo<T>` |
| "Anything readable as a `T`", as a prop | `Signal<T>` |
| A side effect on change | `RenderEffect` |
| Cleanup when the owner goes away | `on_cleanup_local` |
| Schema-shaped state with fine-grained fields | `Store` |
| "Is *this* the selected one?", for a large list | `Selector` |
| A value passed down without threading it through | `provide_context` / `use_context` |

`Selector` is worth knowing about: it turns "which of these thousand rows is selected" into one
subscription per row that fires only for the two rows that changed, instead of a thousand
subscriptions on the selection signal.

Reading and writing are **trait** methods, not inherent ones, so a component can accept "anything
readable as a `T`" without caring whether it was handed a signal, a memo, a store field or a
constant. Those traits must be in scope, which is what `zgui::prelude` is for.

Every read method has an `_untracked` counterpart. Reach for it only where not subscribing is the
point. A read that should have been tracked and was not produces a view that renders once and then
never changes again, with nothing to see in a debugger.

## The three `Send` escapes

The underlying reactive engine is thread-safe by default: a signal's value must be `Send + Sync`, a
context value must be `Send + Sync`, and a cleanup closure must be `Send + Sync`.

The view layer is none of those things. A node handle, a reference-counted callback and a backend
handle are all `Rc`-based, on purpose, because they never leave the UI thread. So there are exactly
three escapes, one per requirement. If a build has stopped on a `Send`/`Sync` bound somewhere near a
signal, the fix is one of these three and nothing else.

### 1. A signal whose value is not `Send`: `LocalStorage`

Every handle type is generic over its storage, and the default is the thread-safe one. A signal
carrying anything from the view layer is written with `LocalStorage` and constructed with
`new_local`:

```rust,ignore
use std::rc::Rc;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

let handle: RwSignal<Rc<str>, LocalStorage> = RwSignal::new_local("a".into());
assert_eq!(&*handle.get(), "a");
```

The choice is a promise checked at run time rather than at compile time: reading a local-storage
signal from another thread panics with a message naming the thread, instead of being undefined.

### 2. A context value that is not `Send`: `provide_local_context`

```rust,ignore
use zgui::reactive::{provide_local_context, use_local_context};

provide_local_context(MyPanelHandle(node));
let handle = use_local_context::<MyPanelHandle>();
```

`provide_context` requires `Send + Sync`; `provide_local_context` does not. Use a **newtype** for
anything whose type is not already specific — `String` or `bool` as a context key collides with
every other use of that type in the process.

### 3. A cleanup closure that is not `Send`: `on_cleanup_local`

The engine's own `on_cleanup` requires a `Send + Sync` closure and therefore cannot capture a node
handle, which is what a cleanup usually needs to capture.

```rust,ignore
use zgui::reactive::on_cleanup_local;

on_cleanup_local(move || host.cancel_timer(timer));
```

## Four things deliberately not published

Each of these fails *silently* rather than loudly, which is why they are not re-exported:

- **Unkeyed store indexing.** It re-runs every sibling's observers, and panics on a stale index when
  the collection shrinks. Key your collections and address them by key.
- **`Effect`.** Its thread-safe constructors run effects off the UI thread. `RenderEffect` is the
  replacement: its first run is synchronous and its lifetime is its handle's.
- **The engine's own `on_cleanup`.** See above; `on_cleanup_local` is the replacement.
- **The engine's `nightly` feature**, which changes how signals are called and would make every
  example in this documentation wrong on one build and right on another.

There is also a startup canary, `effects_are_enabled`, for the one configuration mistake that makes
a whole application compile, run, and never update: an underlying engine built without effects.

## Common mistakes

**A missing `move ||`.** `{count.get()}` is a value; `{move || count.get()}` is a reactive hole.
Both compile. The first never updates.

**Reading a signal outside a tracking context.** A read that is not inside an effect, a memo or a
reactive hole subscribes to nothing. Where that is intended — reading a value once, at setup — use
the `_untracked` form so the intent is stated. Where a whole region should not track, wrap it in a
non-reactive zone.

**Creating a signal outside an owner.** In debug builds this asserts. In release it leaks and
nothing works. Signals belong inside the component body, not in a `static` or a lazily initialised
global.

**Expecting a write to take effect immediately.** Writes settle at the flush. Within one listener,
read back through the same handle if you need the value you just wrote; do not expect the *document*
to have changed yet.

**Holding a `RenderEffect` handle nowhere.** An effect's lifetime is its handle's. Store it in the
state the view keeps, which is what the view layer does for you when the effect comes from a
reactive hole.
