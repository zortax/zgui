//! Sending a state to another thread must not compile.

use zgui_xkb::{Context, RuleNames};

fn main() {
    let context = Context::new().expect("this fixture never runs");
    let keymap = context.keymap(&RuleNames::default()).expect("nor this");
    let mut state = keymap.state().expect("nor this");
    std::thread::spawn(move || {
        state.release(zgui_xkb::Keycode::from_evdev(30));
    });
}
