//! The engine's own cleanup registration requires a closure that can cross threads, so the first
//! cleanup capturing anything from a view fails to compile. `on_cleanup_local` is the published
//! alternative.

fn main() {
    zgui_reactive::on_cleanup(|| ());
}
