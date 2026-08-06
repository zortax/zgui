//! How one group of tokens is declared.
//!
//! A token group is the same shape six times over: a struct of named CSS values, a light default,
//! a dark default, the custom property each field is written as, and a lowering that writes them
//! all out. Written by hand that is six near-identical files whose differences are hard to see and
//! easy to get wrong — a field that lowers under another field's property name is a theme that
//! silently paints the wrong thing.
//!
//! So it is written once, here, and each group is a table.

/// Declares one group of design tokens.
///
/// Expands to the struct, its light and dark defaults, the custom property each field lowers to,
/// and the lowering itself. Every field is a CSS value as text, because a token *is* a CSS value:
/// an application overriding one writes what it would write in a style sheet, and anything the
/// engine can parse is expressible without this crate knowing about it.
///
/// One invocation per module: the expansion brings the store engine's name into scope, and two in
/// one module would collide.
macro_rules! group {
    (
        $(#[$meta:meta])*
        $name:ident, prefix = $prefix:literal, {
            $(
                $(#[$field_meta:meta])*
                $field:ident => $property:literal, light = $light:literal, dark = $dark:literal;
            )+
        }
    ) => {
        // The derives below expand to code that names the store engine, and that code lands in
        // this crate rather than in the engine's — so the name has to be reachable from here.
        #[allow(unused_imports)]
        use ::zgui::reactive::store::reactive_stores;

        $(#[$meta])*
        ///
        /// Every field is a CSS value written as text. Setting one to something the style engine
        /// cannot parse drops that one declaration and leaves the rest of the theme standing,
        /// exactly as a style sheet does.
        #[derive(Clone, Debug, PartialEq, Eq, ::zgui::reactive::Store, ::zgui::reactive::Patch)]
        pub struct $name {
            $(
                $(#[$field_meta])*
                pub $field: ::std::string::String,
            )+
        }

        impl $name {
            /// The custom property each token is written as, in field order.
            ///
            /// This is the list an application overrides against: writing any of these in its own
            /// style sheet replaces that token for everything below the rule it wrote it in.
            pub const PROPERTIES: &'static [&'static str] = &[
                $( ::core::concat!("--zui-", $prefix, "-", $property), )+
            ];

            /// The group's light-scheme defaults.
            pub fn light() -> Self {
                Self { $( $field: ::std::string::String::from($light), )+ }
            }

            /// The group's dark-scheme defaults.
            pub fn dark() -> Self {
                Self { $( $field: ::std::string::String::from($dark), )+ }
            }

            /// Writes every token in the group as a custom-property declaration.
            pub fn declare(&self, out: &mut $crate::Declarations) {
                $( out.push(
                    ::core::concat!("--zui-", $prefix, "-", $property),
                    &self.$field,
                ); )+
            }

            /// Every token as a `(property, value)` pair, in field order.
            pub fn pairs(&self) -> ::std::vec::Vec<(&'static str, &str)> {
                ::std::vec![ $( (
                    ::core::concat!("--zui-", $prefix, "-", $property),
                    self.$field.as_str(),
                ), )+ ]
            }

            /// Sets the token written as `property`, and answers whether this group has one.
            ///
            /// The half of the schema that lets a theme be *read* rather than only written:
            /// [`declare`](Self::declare) turns a group into declarations, and this turns a
            /// declaration back into a token. What it is for is a theme somebody authored as CSS.
            pub fn set(&mut self, property: &str, value: &str) -> bool {
                match property {
                    $(
                        ::core::concat!("--zui-", $prefix, "-", $property) => {
                            self.$field = ::std::string::String::from(value);
                            true
                        }
                    )+
                    _ => false,
                }
            }
        }

        impl ::core::default::Default for $name {
            fn default() -> Self {
                Self::light()
            }
        }

        #[cfg(test)]
        mod group_tests {
            use super::$name;

            #[test]
            fn every_token_lowers_to_its_own_property_and_none_is_written_twice() {
                let mut names = $name::PROPERTIES.to_vec();
                let declared = names.len();
                names.sort_unstable();
                names.dedup();
                assert_eq!(names.len(), declared, "two tokens share a custom property");
            }

            #[test]
            fn the_two_schemes_declare_the_same_tokens() {
                let light: Vec<&str> = $name::light().pairs().iter().map(|pair| pair.0).collect();
                let dark: Vec<&str> = $name::dark().pairs().iter().map(|pair| pair.0).collect();
                assert_eq!(light, dark);
                assert_eq!(light, $name::PROPERTIES);
            }

            #[test]
            fn lowering_writes_one_declaration_per_token() {
                let mut declarations = $crate::Declarations::new();
                $name::light().declare(&mut declarations);
                assert_eq!(declarations.len(), $name::PROPERTIES.len());
                for property in $name::PROPERTIES {
                    assert!(
                        declarations.as_str().contains(property),
                        "{property} is missing from the lowering"
                    );
                }
            }
        }
    };
}

pub(crate) use group;
