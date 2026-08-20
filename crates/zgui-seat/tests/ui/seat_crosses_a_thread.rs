//! Sending a seat to another thread must not compile.

use zgui_seat::Seat;

fn main() {
    let seat = Seat::open().expect("this fixture never runs");
    std::thread::spawn(move || {
        let _ = seat.name();
    });
}
