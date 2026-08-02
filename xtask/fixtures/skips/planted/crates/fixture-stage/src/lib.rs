//! A stage that performs some work and avoids some.

/// Does the work, reusing what it can.
pub fn run() {
    counter::bump(Counter::Alpha);
    counter::add(Counter::Beta, 2);
}
