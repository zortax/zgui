//! A stage that leaves one of the declared counters with no producer at all.
//!
//! `Beta` is named here in four ways that never move it — a doc example, a read, a test-only method
//! and a unit test — which is exactly how a counter comes to look wired while reading zero forever.
//! The test-only method sits inside an `impl`, above the method that increments `Alpha`: removing
//! it must keep the increment below it.
//!
//! It also increments `DrawCalls`, an ordinary declared counter that produces no violation.
//!
//! ```
//! counter::bump(Counter::Beta);
//! ```

/// The stage.
pub struct Stage;

impl Stage {
    /// Moves `Beta` for a test.
    #[cfg(test)]
    fn pretend(&self) {
        counter::bump(Counter::Beta);
    }

    /// Does the work.
    pub fn run(&self) {
        counter::bump(Counter::Alpha);
        let _held = counter::get(Counter::Beta);
        counter::bump(Counter::DrawCalls);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn moves_beta() {
        counter::bump(Counter::Beta);
    }
}
