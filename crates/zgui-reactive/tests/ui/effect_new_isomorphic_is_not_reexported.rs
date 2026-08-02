//! An isomorphic effect exists to run on a server as well as a client, which this framework has
//! no notion of. `RenderEffect` is the published alternative.

fn main() {
    zgui_reactive::Effect::new_isomorphic(|_: Option<()>| ());
}
