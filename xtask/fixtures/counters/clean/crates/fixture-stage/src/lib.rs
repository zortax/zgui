//! A stage that increments every counter the set declares.
//!
//! A test-only method sits above the method that increments `Beta`, and the increment still counts.

/// The stage.
pub struct Stage;

impl Stage {
    /// A helper for tests.
    #[cfg(test)]
    fn helper(&self) {}

    /// Does the work.
    pub fn run(&self) {
        counter::bump(Counter::Alpha);
        counter::add(
            Counter::Beta,
            2,
        );
    }
}
