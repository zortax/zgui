//! How a clipboard answer is matched to the request that asked for it.

use core::fmt::{self, Debug};
use core::sync::atomic::{AtomicU64, Ordering};

/// Which clipboard read an answer belongs to.
///
/// Reads are answered out of band, so an answer has to say which question it is answering. Two
/// paste requests in flight at once is not a contrived case — a slow selection owner and an
/// impatient user produce it — and an answer with no identity would be applied to whichever
/// request happened to be remembered.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipboardSerial(u64);

impl ClipboardSerial {
    /// The identifier with the given raw value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl Debug for ClipboardSerial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ClipboardSerial({})", self.0)
    }
}

/// Hands out identifiers that are never reused.
///
/// A backend keeps one of these and takes an identifier per read. It is shareable and thread-safe
/// because a read can be started from anywhere.
#[derive(Debug)]
pub struct ClipboardSerials {
    next: AtomicU64,
}

impl Default for ClipboardSerials {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardSerials {
    /// A fresh source, starting from the first identifier.
    ///
    /// The first identifier is one and not zero, so that a backend can keep a zero to mean "no
    /// read is outstanding" without that value ever colliding with a real one.
    pub const fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }

    /// The next identifier, which no other call will return.
    pub fn take(&self) -> ClipboardSerial {
        ClipboardSerial(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::{ClipboardSerial, ClipboardSerials};
    use std::collections::BTreeSet;

    #[test]
    fn identifiers_are_never_handed_out_twice() {
        let serials = ClipboardSerials::new();
        let taken: BTreeSet<ClipboardSerial> = (0..1_000).map(|_| serials.take()).collect();
        assert_eq!(taken.len(), 1_000);
    }

    #[test]
    fn both_ways_of_making_a_source_start_at_the_same_identifier() {
        // A derived default would start one lower than the named constructor, which is how a
        // backend ends up with a real read numbered zero after having reserved zero for "none".
        assert_eq!(
            ClipboardSerials::default().take(),
            ClipboardSerials::new().take()
        );
        assert_eq!(ClipboardSerials::default().take(), ClipboardSerial::new(1));
    }

    #[test]
    fn identifiers_are_handed_out_in_order() {
        let serials = ClipboardSerials::new();
        let first = serials.take();
        let second = serials.take();
        assert!(second > first);
    }

    #[test]
    fn a_source_can_be_shared_across_threads() {
        let serials = std::sync::Arc::new(ClipboardSerials::new());
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let serials = std::sync::Arc::clone(&serials);
                std::thread::spawn(move || (0..250).map(|_| serials.take()).collect::<Vec<_>>())
            })
            .collect();
        let mut all = BTreeSet::new();
        for handle in handles {
            all.extend(handle.join().expect("the thread finished"));
        }
        assert_eq!(all.len(), 1_000);
    }
}
