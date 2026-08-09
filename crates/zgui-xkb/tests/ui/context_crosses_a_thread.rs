//! Sending a context to another thread must not compile.

use zgui_xkb::Context;

fn main() {
    let context = Context::new().expect("this fixture never runs");
    std::thread::spawn(move || {
        let _ = context.library();
    });
}
