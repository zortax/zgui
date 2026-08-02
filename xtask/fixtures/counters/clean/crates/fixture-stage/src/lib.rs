//! A stage that increments every counter the set declares.

/// Does the work.
pub fn run() {
    counter::bump(Counter::Alpha);
    counter::add(
        Counter::Beta,
        2,
    );
}
