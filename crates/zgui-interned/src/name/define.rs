//! The one definition every interned name newtype is generated from.

/// Declares a newtype over [`Atom`](crate::Atom) with the full set of traits an interned name
/// needs.
///
/// Every name type in this crate is the same eight bytes with the same operations; what differs
/// is only what the string means, which is exactly what a newtype is for. Writing them out one at
/// a time would put five copies of the same twelve impls in the tree, and a divergence between
/// two of them would be invisible.
macro_rules! interned_name {
    (
        $(#[$attribute:meta])*
        $name:ident
    ) => {
        $(#[$attribute])*
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        #[repr(transparent)]
        pub struct $name($crate::Atom);

        impl $name {
            #[doc = concat!("The `", stringify!($name), "` for `text`, interning it if it has not been seen before.")]
            pub fn new(text: &str) -> Self {
                Self($crate::Atom::new(text))
            }

            #[doc = concat!("The `", stringify!($name), "` for an atom that has already been interned.")]
            pub const fn from_atom(atom: $crate::Atom) -> Self {
                Self(atom)
            }

            /// The underlying atom, for a caller that needs to compare or store names of
            /// different kinds side by side.
            pub const fn atom(self) -> $crate::Atom {
                self.0
            }

            /// The interned text.
            pub fn as_str(self) -> &'static str {
                self.0.as_str()
            }

            /// Whether two names are the very same interned string.
            pub fn is(self, other: Self) -> bool {
                self.0.is(other.0)
            }

            /// Whether the interned text is empty.
            pub fn is_empty(self) -> bool {
                self.0.is_empty()
            }

            /// The length of the interned text in bytes.
            pub fn len(self) -> usize {
                self.0.len()
            }
        }

        impl ::core::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter
                    .debug_tuple(::core::stringify!($name))
                    .field(&self.as_str())
                    .finish()
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl ::core::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl ::core::convert::From<&str> for $name {
            fn from(text: &str) -> Self {
                Self::new(text)
            }
        }

        impl ::core::convert::From<::std::string::String> for $name {
            fn from(text: ::std::string::String) -> Self {
                Self::new(&text)
            }
        }

        impl ::core::convert::From<$crate::Atom> for $name {
            fn from(atom: $crate::Atom) -> Self {
                Self(atom)
            }
        }

        impl ::core::convert::From<$name> for $crate::Atom {
            fn from(name: $name) -> Self {
                name.0
            }
        }

        impl ::core::cmp::PartialEq<str> for $name {
            fn eq(&self, text: &str) -> bool {
                self.as_str() == text
            }
        }

        impl ::core::cmp::PartialEq<&str> for $name {
            fn eq(&self, text: &&str) -> bool {
                self.as_str() == *text
            }
        }
    };
}

pub(crate) use interned_name;
