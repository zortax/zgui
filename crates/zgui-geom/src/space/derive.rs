//! Standard trait implementations for the space-tagged geometry types.
//!
//! `#[derive]` would put a bound on the space parameter — `S: Clone`, `S: Debug` and so on — and
//! the space markers are uninhabited types that exist only to be named, so those bounds are both
//! unmeetable in spirit and wrong: whether a rectangle can be copied depends on its scalar, never
//! on which coordinate system it was measured in. The implementations here are the derived ones
//! with the space parameter left unconstrained.

/// Implements the standard traits for a type shaped `Name<T, S>`.
///
/// The first form is for a type that carries a `space: PhantomData<S>` field of its own; the
/// second is for one whose fields already carry the space.
macro_rules! space_derives {
    ($name:ident { $($field:ident),+ $(,)? }) => {
        $crate::space::derive::space_impls!($name { $($field),+ } marker { space: ::core::marker::PhantomData });
    };
    ($name:ident { $($field:ident),+ $(,)? } tagged by its fields) => {
        $crate::space::derive::space_impls!($name { $($field),+ } marker { });
    };
}

/// The shared body of [`space_derives`], with the marker field spliced in where a type has one.
macro_rules! space_impls {
    ($name:ident { $($field:ident),+ } marker { $($marker:tt)* }) => {
        impl<T: Clone, S> Clone for $name<T, S> {
            fn clone(&self) -> Self {
                Self {
                    $($field: self.$field.clone(),)+
                    $($marker)*
                }
            }
        }

        impl<T: Copy, S> Copy for $name<T, S> {}

        impl<T: ::core::fmt::Debug, S: $crate::space::Space> ::core::fmt::Debug for $name<T, S> {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter
                    .debug_struct(::core::stringify!($name))
                    $(.field(::core::stringify!($field), &self.$field))+
                    .field("space", &<S as $crate::space::Space>::NAME)
                    .finish()
            }
        }

        impl<T: Default, S> Default for $name<T, S> {
            fn default() -> Self {
                Self {
                    $($field: ::core::default::Default::default(),)+
                    $($marker)*
                }
            }
        }

        impl<T: PartialEq, S> PartialEq for $name<T, S> {
            fn eq(&self, other: &Self) -> bool {
                $(self.$field == other.$field)&&+
            }
        }

        impl<T: Eq, S> Eq for $name<T, S> {}

        impl<T: ::core::hash::Hash, S> ::core::hash::Hash for $name<T, S> {
            fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                $(::core::hash::Hash::hash(&self.$field, state);)+
            }
        }
    };
}

pub(crate) use {space_derives, space_impls};
