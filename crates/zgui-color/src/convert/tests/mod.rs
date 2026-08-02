//! Conversion tests: known reference colours, and reversibility across every space.

mod reference;
mod roundtrip;

/// Asserts that two channel triples agree to within `tolerance`, naming the case on failure.
pub(crate) fn assert_close(actual: [f32; 3], expected: [f32; 3], tolerance: f32, what: &str) {
    for channel in 0..3 {
        assert!(
            (actual[channel] - expected[channel]).abs() <= tolerance,
            "{what}: channel {channel} is {}, expected {} (tolerance {tolerance})",
            actual[channel],
            expected[channel],
        );
    }
}
