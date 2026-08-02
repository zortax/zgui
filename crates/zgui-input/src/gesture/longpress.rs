//! How long a contact has to be held before it means something else.

use core::time::Duration;

/// How long a still contact is held before it becomes a long press.
///
/// Half a second is what a touch platform means by "press and hold": short enough that nobody
/// wonders whether it worked, long enough that it is not fired by a slow tap.
pub const LONG_PRESS: Duration = Duration::from_millis(500);

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::LONG_PRESS;

    #[test]
    fn it_is_longer_than_a_slow_tap_and_shorter_than_a_pause() {
        assert!(LONG_PRESS > Duration::from_millis(250));
        assert!(LONG_PRESS < Duration::from_millis(1_000));
    }
}
