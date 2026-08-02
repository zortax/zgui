//! A stage that leaves one of the declared counters with no producer at all.
//!
//! `Beta` is named here in three ways that never move it — a doc example, a read, and a unit test —
//! which is exactly how a counter comes to look wired while reading zero forever.
//!
//! It also increments `DrawCalls`, which the checker still lists as awaiting the renderer. That is
//! the opposite violation: a promise kept without being retired, which is how the list of counters
//! awaiting a stage stops being a list of the counters that are actually missing one.
//!
//! ```
//! counter::bump(Counter::Beta);
//! ```

/// Does the work.
pub fn run() {
    counter::bump(Counter::Alpha);
    let _held = counter::get(Counter::Beta);
    counter::bump(Counter::DrawCalls);
}

#[cfg(test)]
mod tests {
    #[test]
    fn moves_beta() {
        counter::bump(Counter::Beta);
    }
}
