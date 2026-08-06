# Doing something that takes time

An interface spends most of its life waiting: for a response, for a file, for a decode, for a
worker. This chapter is about where that waiting happens, what it is allowed to touch, and what
stops it when the view it belongs to goes away.

There are three places work can run, and one function for each.

| | Runs on | May touch signals | Cancelled by |
|---|---|---|---|
| `spawn(future)` | the UI thread, at the flush | yes | the scope that spawned it |
| `background(future)` / `blocking(closure)` | a worker | **no** | nothing — see below |
| `ui().post(closure)` | the UI thread, at the flush | yes | nothing |

## The shape almost everything takes

```rust,ignore
view! {
    control(on:click = move |_| {
        spawn(async move {
            loading.set(true);
            let rows = background(async { fetch().await }).await;
            items.set(rows);
            loading.set(false);
        });
    }) { "Reload" }
}
```

One `spawn`, one `await`, and the thread boundary crossed twice inside it. The lines around the
`await` run on the UI thread, which is why `loading.set` and `items.set` are legal there; only the
future handed to `background` moves. When it finishes, the result comes back through the executor's
wake edge, which asks the platform for the frame that delivers it — the same edge a timer or a
signal written from a worker thread uses, so it costs one frame, not one per write.

`background` is for work that is already a future. `blocking` is for work that is not: a decode, a
parse, a database call through a synchronous driver.

## What runs where

**`spawn` and `spawn_local` run on the UI thread.** They are polled inside `flush()`, in the
reactive phase of the frame. That is what makes them allowed to touch the document, node handles
and view state — and it is why a future that *works* rather than *waits* must not be one of these.
Waiting on the UI thread is free; ten milliseconds of parsing there is a dropped frame.

`spawn` and `spawn_local` differ only in whether the future has to be `Send`. Neither moves it to
another thread. `spawn` asks for `Send` so it can stand in for a thread-pool spawn in generic code;
if the future captures anything from the view layer, use `spawn_local`.

**`background` and `blocking` run on a worker.** Both the future and its output must be `Send`,
because both cross a thread boundary, and neither may touch anything reactive. Read the signals you
need *before* the call and move the values in:

```rust,ignore
let id = selected.get();                        // on the UI thread, and tracked
let detail = background(async move { load(id).await }).await;
```

That ordering matters most inside `AsyncDerived` and `Action`, whose closures re-run when their
reactive inputs change. The reads that register those inputs happen in the closure body, so moving
the body wholesale onto a worker registers nothing and the value is computed once and never again.

**`ui()` posts back onto the UI thread.** A `Ui` handle is `Send + Clone`, so it goes wherever the
work goes — a `std::thread`, a background task, a callback a C library invokes on a thread of its
own. `post` queues a closure that runs at the start of the next flush and asks for that frame;
`run` is the same thing awaited, for a background task that needs an answer before it carries on.

Signals are `Send`, so a worker *can* write one directly. It should not: `Observer` and `Owner` are
thread-local, so a `get` on a worker silently fails to subscribe, and one refactor later the same
code touches a `LocalStorage` handle and panics. Post instead.

## Cancellation

A task is cancelled when the scope that spawned it is disposed of. Nothing has to be registered for
this and nothing has to be remembered — a task spawned in a component's body or from any of its
listeners belongs to that component, and unmounting it stops the task and drops what the future
captured, synchronously, inside the unmount.

That last word is the load-bearing one. A task whose captures outlive its owner by a frame is
holding node handles for a node that no longer exists, which is exactly what the owner tree exists
to rule out.

**Dropping the handle does not cancel the task.** This is deliberate, and it is the one place tasks
differ from the framework's other handles — `TimeoutHandle` and `IntervalHandle` do cancel on drop.
A timer is a standing registration whose handle is the only thing naming it; a task is a piece of
work, and the form nine callers in ten write is:

```rust,ignore
on:click = move |_| { spawn(async { … }); }
```

where the handle dies at the semicolon. Cancelling there would mean nothing ever ran. Keep the
`Task` if you want a Cancel button; ignore it otherwise.

`spawn_detached` is the escape for work that must finish whatever happens to the view — a save on
the way out, an acknowledgement, a metric.

**Cancelling stops the task, not the worker.** `background` hands a future to another thread, and
there is no way to interrupt it once it is running. Cancelling the UI-side task drops the result
when it arrives; it does not shorten the work.

**A late result is still your problem.** Cancellation covers the view going away. It does not cover
the view still being there and wanting something else — a second click, a different page. Re-read
what you asked for after the `await` and drop the result if it no longer matches:

```rust,ignore
let wanted = page.get_untracked();
let loaded = background(async move { fetch_page(wanted) }).await;
if page.get_untracked() == wanted {
    rows.set(loaded);
}
```

## Streams and channels

`spawn_stream(stream, |item| …)` runs a stream as a UI-thread task, delivering each item inside a
flush. `signal_from_stream(initial, stream)` is the same thing collapsed into a signal holding the
latest item. Both are cancelled with their scope, which is what stops a subscription outliving the
view showing it.

Nothing there needs a runtime. `futures` channels work as they are, and so does `tokio::sync` —
`mpsc`, `watch`, `broadcast` and `oneshot` are all runtime-agnostic and can be awaited on the UI
thread's pool with no tokio installed at all.

## tokio

The default background executor is a small pool — at most four threads, started the first time
anything asks for one, with no reactor. It is enough for a decode or a parse and costs nothing
until used.

What it cannot do is run the parts of tokio that need a *runtime context* rather than merely a
thread: `tokio::time`, `tokio::net`, `tokio::fs`, and everything built on them. Those panic when
constructed outside a runtime, however correctly they are afterwards polled.

`zgui-tokio` closes that gap. It is a separate crate behind an off-by-default `tokio` feature, so a
program that does not want a multi-threaded runtime does not link one:

```toml
zgui = { version = "0.1", features = ["tokio"] }
```

```rust,ignore
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tokio = zgui::tokio::install()?;
    app().with_title("…").run(|| view! { App() })
}
```

Installing does two things. Background work goes to the runtime, so a tokio-based library works
inside `background` with no further ceremony, and `blocking` reaches tokio's own blocking pool.
And the UI thread's task pool is polled *inside* the runtime, which means a UI-thread task may
await a tokio timer or a socket directly:

```rust,ignore
spawn(async move {
    tokio::time::sleep(Duration::from_secs(3)).await;
    toast.set(None);
});
```

The cost of that convenience is that awaiting something slow on the UI thread becomes easy to do by
accident. The rule has not changed: waiting there is free, working there is not.

`install_handle` adopts a runtime the application already owns. `enter_here` gives a second UI
thread the same reach without installing a second runtime — the background executor is one per
process, while entering the runtime is a property of one thread's flush.

Installing tokio does not compete with zgui for `any_spawner`'s process-wide executor slot; zgui
claims that with its own UI-thread executor and `zgui-tokio` never touches it.

## A worked example

`examples/async.rs` — `cargo run -p zgui-examples --example async` — is a paged list that loads
through `background`, reports progress from a plain `std::thread` through `ui().post`, cancels in
flight, and guards against the stale result described above.

## See also

- [The reactive model](reactivity.md) — signals, owners, the flush, and the three `Send` escapes.
- [The architecture](architecture.md) — where the reactive phase sits in the frame.
