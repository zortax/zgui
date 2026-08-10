//! Sharing a seat between threads must not compile.

use zgui_seat::Seat;

fn shared<T: Sync>(_: &T) {}

fn main() {
    let seat = Seat::open().expect("this fixture never runs");
    shared(&seat);
}
