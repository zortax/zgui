//! A thread-safe effect can be scheduled off the UI thread, where it may not touch the
//! document. `RenderEffect` is the published alternative.

fn main() {
    zgui_reactive::Effect::new_sync(|_: Option<()>| ());
}
