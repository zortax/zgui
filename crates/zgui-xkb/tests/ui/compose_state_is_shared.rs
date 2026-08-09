//! Sharing a compose state between threads must not compile.

use zgui_xkb::{ComposeState, Context};

fn shared<T: Sync>(_: &T) {}

fn main() {
    let context = Context::new().expect("this fixture never runs");
    let table = context.compose_table("C").expect("nor this");
    let state: ComposeState = table.state().expect("nor this");
    shared(&state);
}
