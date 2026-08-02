//! The one declaration the counter enum, its metadata and the snapshot struct are built from.

/// Declares the whole counter set.
///
/// Each entry gives the variant name, the snapshot field name and the group, and carries the doc
/// comment that becomes documentation for both the variant and the field. One declaration means a
/// counter cannot exist in the enum but be missing from a snapshot, or be described one way in one
/// place and another way in the other.
macro_rules! counters {
    (
        $(
            $(#[$attribute:meta])*
            $variant:ident => $field:ident, $group:expr;
        )+
    ) => {
        /// One measured quantity of a frame.
        ///
        /// Each counter records work actually performed, so a stage that decided to do nothing
        /// leaves its counters where it found them. Two of them —
        /// [`Counter::NodesVisited`] and [`Counter::DirtyWalkSteps`] — are the exception, and
        /// exist precisely because every other counter would be silent about a traversal that
        /// walked the whole document to do one node's work.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        #[repr(usize)]
        pub enum Counter {
            $(
                $(#[$attribute])*
                $variant,
            )+
        }

        impl Counter {
            /// How many counters there are.
            pub const COUNT: usize = [$(counters!(@unit $variant)),+].len();

            /// Every counter, in declaration order.
            pub const ALL: [Counter; Self::COUNT] = [$(Counter::$variant),+];

            /// The counter's name, spelled the way its snapshot field is.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => ::core::stringify!($field),)+
                }
            }

            /// Whether the counter means the same thing under every renderer.
            pub const fn group(self) -> $crate::counter::Group {
                match self {
                    $(Self::$variant => $group,)+
                }
            }

            /// The counter's position in a snapshot, which is its declaration order.
            pub const fn index(self) -> usize {
                self as usize
            }
        }

        /// Every counter's value at one moment, read out together.
        ///
        /// The values are read one at a time rather than atomically as a set, so a snapshot taken
        /// while a frame is in flight can hold values from slightly different instants. Taken
        /// between frames — which is what a test does — it is exact.
        #[derive(Copy, Clone, PartialEq, Eq, Default)]
        pub struct Counters {
            $(
                $(#[$attribute])*
                pub $field: u64,
            )+
        }

        impl Counters {
            /// A snapshot in which every counter reads zero.
            pub const ZERO: Self = Self { $($field: 0,)+ };

            /// Builds a snapshot by asking `read` for each counter in turn.
            pub fn from_fn(mut read: impl FnMut(Counter) -> u64) -> Self {
                Self { $($field: read(Counter::$variant),)+ }
            }

            /// One counter's value.
            pub const fn get(&self, counter: Counter) -> u64 {
                match counter {
                    $(Counter::$variant => self.$field,)+
                }
            }

            /// Every counter and its value, in declaration order.
            pub fn iter(&self) -> impl ExactSizeIterator<Item = (Counter, u64)> + '_ {
                Counter::ALL.into_iter().map(|counter| (counter, self.get(counter)))
            }

            /// The counters that moved between `self` and a later snapshot.
            pub fn delta(&self, later: &Self) -> Self {
                Self { $($field: later.$field.saturating_sub(self.$field),)+ }
            }
        }

        impl ::core::fmt::Debug for Counters {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let mut list = formatter.debug_struct("Counters");
                for (counter, value) in self.iter() {
                    if value != 0 {
                        list.field(counter.name(), &value);
                    }
                }
                list.finish_non_exhaustive()
            }
        }
    };

    (@unit $variant:ident) => { () };
}

pub(crate) use counters;
